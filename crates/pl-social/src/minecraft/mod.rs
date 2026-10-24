//! SKIN1 (skin1.md): the Projects skin of username accounts ("Oyuncu adıyla oyna") and the game
//! account's link to the player's Discord account, from the launcher, through the friends service
//! (`/v1/mc`; README "Minecraft link and Projects skin"). ProBot links and Lobi applies, so a saved
//! skin shows at the player's next login on Projects.
//!
//! - [`Session`]'s `mc_*` calls: the routes, each failure with its details ([`McFailure`]).
//! - [`ProjectsSkins`]: what the launcher runs. It reads the service only when asked (the panel
//!   opens, a change is sent, at most once a minute for the entry points) or told (the socket's
//!   `mc` messages). A job it sent is asked about on its own only while the socket is down: every
//!   3 s for 30 s, then every 10 s up to 5 minutes. Picked skins, Mojang's answers and this
//!   computer's copies stay here; the interface gets views ([`ProjectsSkinView`] …) and changes
//!   ([`ProjectsSkinChange`]).
//!
//! [`Session`]: crate::Session

mod codeless;
mod store;
mod view;
mod wire;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pl_auth::{
    CleanSkin, DefaultSkin, MojangEndpoints, MojangLookup, PremiumLookup, PremiumSkin, SkinVariant,
    skin_preview,
};
use tokio::runtime::Handle;

use crate::client::Session;
use crate::service::SocialService;
use crate::view::SkinBlockView;

pub use store::KEPT_UNUSED;
use store::{KeptSource, SkinStore, Slot};
use view::{
    McNews, access_of, access_of_state, data_url, job_view, link_view, news_of, pick_refusal,
    refusal_of, skin_view,
};
pub use view::{
    ProjectsLink, ProjectsLinkState, ProjectsSkin, ProjectsSkinAccess, ProjectsSkinBy,
    ProjectsSkinChange, ProjectsSkinChoice, ProjectsSkinChoiceKind, ProjectsSkinJob,
    ProjectsSkinJobKind, ProjectsSkinJobState, ProjectsSkinKind, ProjectsSkinLimited,
    ProjectsSkinLimits, ProjectsSkinLook, ProjectsSkinLookup, ProjectsSkinOutcome,
    ProjectsSkinOverview, ProjectsSkinPick, ProjectsSkinPickRefusal, ProjectsSkinRefusal,
    ProjectsSkinRefusalCode, ProjectsSkinResult, ProjectsSkinResultCode, ProjectsSkinSave,
    ProjectsSkinService, ProjectsSkinView,
};
pub use wire::{
    FEATURE_MC_CODELESS, FEATURE_MC_LINK, FEATURE_SKINS, McBlocked, McCheck, McDeclined, McFailure,
    McHealth, McIntent, McJob, McJobKind, McJobResult, McJobState, McLimitedUntil, McLimits,
    McLink, McLinkState, McMessage, McPremium, McSkin, McSkinBy, McSkinKind, McSource, McState,
};

/// The entry points read the service in the background at most this often.
const READ_EVERY: Duration = Duration::from_secs(60);
/// Without the socket, a job sent from here is asked about this often at first…
const POLL_FAST: Duration = Duration::from_secs(3);
const POLL_FAST_FOR: Duration = Duration::from_secs(30);
/// …then this often, up to [`POLL_FOR`].
const POLL_SLOW: Duration = Duration::from_secs(10);
const POLL_FOR: Duration = Duration::from_secs(300);
/// A job that closes sooner than this after it was sent is told only then: the interface that
/// sent it listens once the command has answered.
const SETTLE: Duration = Duration::from_millis(500);
/// Picked skins kept for `save` (the interface keeps one at a time).
const CHOICES_KEPT: usize = 16;
const TOLD_KEPT: usize = 64;
/// Previews of Projects skins fetched from Mojang's texture host.
const TEXTURES_KEPT: usize = 16;

/// What the friends service hands SKIN1 from its presence socket.
pub(crate) enum McSignal {
    Message(Box<McMessage>),
    /// The socket (re)connected: what happened meanwhile is read again.
    Connected,
}

pub(crate) type McSink = Arc<dyn Fn(McSignal) + Send + Sync>;

/// Where `projects-skin-changed` goes.
pub type SkinChangeSink = Arc<dyn Fn(ProjectsSkinChange) + Send + Sync>;

/// A launcher account as SKIN1 needs it: its id and its game name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinAccount {
    pub id: String,
    pub name: String,
}

/// The launcher's accounts right now (a closed job is told with the account of its name).
pub type AccountsFn = Arc<dyn Fn() -> Vec<SkinAccount> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SkinsConfig {
    /// This computer's copies of the accounts' Projects skins (`<data>/skins/projects`).
    pub store_dir: PathBuf,
    /// The launcher's `versions` folder: its client jars hold Minecraft's default skins.
    pub versions_dir: PathBuf,
    pub mojang: MojangEndpoints,
}

/// SKIN1 in the launcher. Cheap to clone.
#[derive(Clone)]
pub struct ProjectsSkins {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ProjectsSkins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectsSkins").finish_non_exhaustive()
    }
}

struct Inner {
    social: SocialService,
    mojang: MojangLookup,
    store: SkinStore,
    versions_dir: PathBuf,
    runtime: Handle,
    sink: SkinChangeSink,
    accounts: AccountsFn,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The signed content has `features.friends` and `features.skins`.
    shown: bool,
    /// The friends session everything below belongs to.
    generation: Option<u64>,
    /// The last `GET /v1/mc`, as `mc` messages changed it since.
    known: Option<McState>,
    /// When the service was last read (or a background read started).
    read_at: Option<Instant>,
    /// Link tries open again then (Unix seconds; after a `429` with `link_tries`).
    link_retry_at: Option<i64>,
    choices: VecDeque<(String, Choice)>,
    next_choice: u64,
    /// Jobs sent from here and not told yet.
    sent: HashMap<String, Sent>,
    /// Closed jobs told already (newest last).
    told: VecDeque<String>,
    /// Previews by texture id.
    textures: VecDeque<(String, String)>,
    pruned: bool,
}

#[derive(Clone)]
enum Choice {
    Png(Arc<CleanSkin>),
    Premium {
        skin: Arc<PremiumSkin>,
        preview: Arc<Vec<u8>>,
    },
}

struct Sent {
    /// The launcher account it was for (skin and link jobs).
    account: Option<SkinAccount>,
    at: Instant,
    /// Skin jobs: the preview (data URL) and the arm model.
    picture: Option<String>,
    variant: Option<SkinVariant>,
}

/// A skin change ready to go.
struct Prepared {
    source: McSource,
    kept: KeptSource,
    /// What this computer keeps: the clean PNG, or the premium player's texture.
    picture: Option<Vec<u8>>,
    preview: Option<Vec<u8>>,
    variant: Option<SkinVariant>,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn offers(features: &[String], feature: &str) -> bool {
    features.iter().any(|f| f == feature)
}

impl ProjectsSkins {
    /// Listens to the friends service's `mc` messages from now on.
    pub fn new(
        social: &SocialService,
        http: reqwest::Client,
        config: SkinsConfig,
        runtime: Handle,
        sink: SkinChangeSink,
        accounts: AccountsFn,
    ) -> Self {
        let inner = Arc::new(Inner {
            social: social.clone(),
            mojang: MojangLookup::with_endpoints(http, config.mojang),
            store: SkinStore::new(config.store_dir),
            versions_dir: config.versions_dir,
            runtime,
            sink,
            accounts,
            state: Mutex::new(State::default()),
        });
        let weak: Weak<Inner> = Arc::downgrade(&inner);
        social.set_mc_sink(Arc::new(move |signal| {
            if let Some(inner) = weak.upgrade() {
                ProjectsSkins { inner }.on_signal(signal);
            }
        }));
        Self { inner }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self, change: ProjectsSkinChange) {
        (self.inner.sink)(change);
    }

    /// The signed content's `features.friends && features.skins` (the entry points).
    pub fn set_shown(&self, shown: bool) {
        self.lock().shown = shown;
    }

    /// The friends session, and what an earlier one left behind gone.
    fn session(&self) -> Option<(Session, u64)> {
        let (session, generation) = self.inner.social.mc_session()?;
        let mut state = self.lock();
        if state.generation != Some(generation) {
            state.generation = Some(generation);
            state.known = None;
            state.read_at = None;
            state.link_retry_at = None;
            state.sent.clear();
            state.told.clear();
        }
        Some((session, generation))
    }

    /// Keeps a read of the service (for this session only).
    fn remember(&self, generation: u64, read: &McState) -> bool {
        let mut state = self.lock();
        if state.generation != Some(generation) {
            return false;
        }
        state.known = Some(read.clone());
        state.read_at = Some(Instant::now());
        true
    }

    /// A refusal's side effects: an ended session ends here too, a `link_tries` wait is kept.
    fn failed(&self, generation: u64, failure: &McFailure) {
        if failure.unauthorized() {
            self.inner.social.mc_unauthorized(generation);
        }
        if failure.status == 429 && failure.item.as_deref() == Some("link_tries") {
            let wait = i64::try_from(failure.retry_after.unwrap_or(3_600)).unwrap_or(3_600);
            let mut state = self.lock();
            if state.generation == Some(generation) {
                state.link_retry_at = Some(unix_now() + wait);
            }
        }
    }

    /// Copies of accounts that are gone go, once a run (not before the accounts are known).
    fn prune_once(&self) {
        if self.lock().pruned {
            return;
        }
        let accounts: Vec<(String, String)> = (self.inner.accounts)()
            .into_iter()
            .map(|account| (account.id, account.name))
            .collect();
        if accounts.is_empty() {
            return;
        }
        self.lock().pruned = true;
        let store = self.inner.store.clone();
        self.inner
            .runtime
            .spawn_blocking(move || store.prune(&accounts, SystemTime::now()));
    }

    // --- what the interface reads -------------------------------------------------------------

    /// The entry points and the link as last known, from memory. While signed in and nothing is
    /// known yet, the service is read in the background (at most once a minute) and a `state`
    /// change follows.
    pub fn overview(&self) -> ProjectsSkinOverview {
        self.prune_once();
        let session = self.session();
        let friends = self.inner.social.friends_features();
        let (overview, read) = {
            let mut state = self.lock();
            let shown = state.shown;
            if session.is_none() {
                return ProjectsSkinOverview {
                    shown,
                    service: None,
                    link: None,
                };
            }
            let features = state
                .known
                .as_ref()
                .map(|known| known.features.clone())
                .or(friends);
            let service = features.as_deref().map(|features| ProjectsSkinService {
                link: offers(features, FEATURE_MC_LINK),
                skins: offers(features, FEATURE_SKINS),
            });
            // An older service (no SKIN1) is never asked.
            let offered = features.as_deref().is_none_or(|features| {
                offers(features, FEATURE_MC_LINK) || offers(features, FEATURE_SKINS)
            });
            let read = shown
                && offered
                && state.known.is_none()
                && state.read_at.is_none_or(|at| at.elapsed() >= READ_EVERY);
            if read {
                state.read_at = Some(Instant::now());
            }
            let overview = ProjectsSkinOverview {
                shown,
                service,
                link: state.known.as_ref().map(|known| link_view(&known.link)),
            };
            (overview, read)
        };
        if read && let Some((session, generation)) = session {
            let this = self.clone();
            self.inner.runtime.spawn(async move {
                this.read_in_background(&session, generation).await;
            });
        }
        overview
    }

    async fn read_in_background(&self, session: &Session, generation: u64) {
        match session.mc_state().await {
            Ok(read) => {
                if self.remember(generation, &read) {
                    self.reconcile(generation, &read);
                    self.emit(ProjectsSkinChange::State);
                }
            }
            Err(failure) => self.failed(generation, &failure),
        }
    }

    /// The Görünüm panel's view of `account`: the service read now, this computer's copy.
    pub async fn state(&self, account: &SkinAccount) -> ProjectsSkinView {
        self.prune_once();
        let Some((session, generation)) = self.session() else {
            return self
                .view(account, ProjectsSkinAccess::SignedOut, None)
                .await;
        };
        match session.mc_state().await {
            Ok(read) => {
                if self.remember(generation, &read) {
                    self.reconcile(generation, &read);
                }
                self.settle_kept(account, &read);
                let access = access_of_state(&read);
                self.view(account, access, Some(read)).await
            }
            Err(failure) => {
                self.failed(generation, &failure);
                let known = self.lock().known.clone();
                self.view(account, access_of(&failure), known).await
            }
        }
    }

    /// A change sent from this computer that closed while nothing here followed it (the launcher
    /// was closed): this computer's copy follows it now. Nothing is told; the panel's `lastJob`
    /// says how it ended.
    fn settle_kept(&self, account: &SkinAccount, read: &McState) {
        let Some(pending) = self.inner.store.read(&account.id, &account.name).pending else {
            return;
        };
        if self.lock().sent.contains_key(&pending.job) {
            return;
        }
        let Some(job) = read
            .jobs
            .iter()
            .find(|job| job.id == pending.job && !job.state.is_open())
        else {
            return;
        };
        if job.applied() {
            let texture = job.result.as_ref().and_then(|r| r.texture.as_deref());
            if let Err(error) =
                self.inner
                    .store
                    .applied(&account.id, &account.name, &job.id, texture)
            {
                tracing::info!(%error, "the applied Projects skin is not kept here");
            }
        } else {
            self.inner
                .store
                .drop_pending(&account.id, &account.name, &job.id);
        }
    }

    async fn view(
        &self,
        account: &SkinAccount,
        access: ProjectsSkinAccess,
        read: Option<McState>,
    ) -> ProjectsSkinView {
        let kept = self.inner.store.read(&account.id, &account.name);
        let read = read.unwrap_or_default();
        let skin = match read
            .skin
            .as_ref()
            .filter(|_| read.link.linked_to(&account.name))
        {
            Some(skin) => Some(skin_view(
                skin,
                self.skin_picture(account, skin, kept.current.as_ref())
                    .await,
            )),
            None => None,
        };
        let (job, last_job, link_job, link_retry_at) = {
            let state = self.lock();
            let sent_view = |job: &McJob| {
                let sent = state.sent.get(&job.id);
                job_view(
                    job,
                    sent.and_then(|sent| sent.picture.clone()),
                    sent.and_then(|sent| sent.variant),
                )
            };
            let mine = |job: &&McJob| job.kind == McJobKind::Skin && job.is_for(&account.name);
            let job = read
                .jobs
                .iter()
                .filter(mine)
                .find(|job| job.state.is_open())
                .and_then(sent_view);
            let last_job = read
                .jobs
                .iter()
                .filter(mine)
                .find(|job| !job.state.is_open())
                .and_then(sent_view);
            let link_job = read
                .jobs
                .iter()
                .find(|job| job.kind.is_link_family() && job.state.is_open())
                .and_then(sent_view);
            let now = unix_now();
            (
                job,
                last_job,
                link_job,
                state.link_retry_at.filter(|at| *at > now),
            )
        };
        let can_undo = kept.previous.as_ref().is_some_and(|previous| {
            !previous.has_picture()
                || self
                    .inner
                    .store
                    .picture(&account.id, Slot::Previous)
                    .is_some()
        });
        ProjectsSkinView {
            account_id: account.id.clone(),
            name: account.name.clone(),
            access,
            server_away: !read.health.server,
            link: link_view(&read.link),
            skin,
            job,
            last_job,
            link_job,
            limits: ProjectsSkinLimits {
                per_day: read.limits.skins_per_day,
                left: read.limits.skins_left,
                reset_at: read.limits.reset_at,
                link_tries_left: read.limits.link_tries_left,
                link_retry_at,
            },
            blocked: read.blocked.map(|blocked| SkinBlockView {
                until: blocked.until,
            }),
            limited: read.limited.map(|limited| ProjectsSkinLimited {
                until: limited.until,
            }),
            can_undo,
            on_projects: self.inner.social.on_projects(),
        }
    }

    /// The Projects skin's picture: this computer's copy when it is that skin, else the texture
    /// from Mojang's texture host by its id (kept for this run).
    async fn skin_picture(
        &self,
        account: &SkinAccount,
        skin: &McSkin,
        current: Option<&KeptSource>,
    ) -> Option<String> {
        if skin.kind == McSkinKind::Default {
            return None;
        }
        let same = current.is_some_and(|current| {
            current.has_picture()
                && match (current.texture(), skin.texture.as_deref()) {
                    (Some(kept), Some(shown)) => kept.eq_ignore_ascii_case(shown),
                    (_, None) => true,
                    (None, Some(_)) => false,
                }
        });
        if same
            && let Some(preview) = self
                .inner
                .store
                .picture(&account.id, Slot::Current)
                .and_then(|png| skin_preview(&png).ok())
        {
            return Some(data_url(&preview));
        }
        let id = skin.texture.clone()?;
        if let Some((_, picture)) = self.lock().textures.iter().find(|(known, _)| *known == id) {
            return Some(picture.clone());
        }
        let png = self.inner.mojang.texture(&id).await?;
        let picture = data_url(&skin_preview(&png).ok()?);
        let mut state = self.lock();
        state.textures.push_back((id, picture.clone()));
        while state.textures.len() > TEXTURES_KEPT {
            state.textures.pop_front();
        }
        Some(picture)
    }

    /// What `account` looks like on Projects without a Projects skin: the skin of a premium
    /// account of the same name, else the default skin the game gives the name (from a client jar
    /// on this computer). None when neither is at hand.
    pub async fn default_look(&self, account: &SkinAccount) -> Option<ProjectsSkinLook> {
        if let PremiumLookup::Found(premium) = self.inner.mojang.lookup(&account.name).await
            && let Ok(preview) = skin_preview(&premium.png)
        {
            return Some(ProjectsSkinLook {
                picture: data_url(&preview),
                variant: premium.variant,
                premium: true,
            });
        }
        let skin = DefaultSkin::for_profile(pl_auth::offline_uuid(&account.name));
        let versions = self.inner.versions_dir.clone();
        let png = self
            .inner
            .runtime
            .spawn_blocking(move || {
                pl_auth::read_default_skin(&pl_auth::client_jars(&versions), skin)
            })
            .await
            .ok()
            .flatten()?;
        Some(ProjectsSkinLook {
            picture: data_url(&skin_preview(&png).ok()?),
            variant: skin.variant,
            premium: false,
        })
    }

    fn keep_choice(&self, choice: Choice) -> String {
        let mut state = self.lock();
        state.next_choice += 1;
        let id = format!("c{}", state.next_choice);
        state.choices.push_back((id.clone(), choice));
        while state.choices.len() > CHOICES_KEPT {
            state.choices.pop_front();
        }
        id
    }

    /// Reads (at most 1 MiB), checks and re-encodes a skin file the player picked.
    pub async fn pick_file(&self, path: PathBuf) -> ProjectsSkinPick {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let read = self
            .inner
            .runtime
            .spawn_blocking(move || pl_auth::read_clean_skin(&path))
            .await;
        match read {
            Ok(Ok(skin)) => {
                let picture = data_url(&skin.preview);
                let (legacy, black_parts) = (skin.legacy, skin.black_parts);
                let id = self.keep_choice(Choice::Png(Arc::new(skin)));
                ProjectsSkinPick::Ready {
                    choice: ProjectsSkinChoice {
                        id,
                        kind: ProjectsSkinChoiceKind::Png,
                        picture,
                        name,
                        variant: SkinVariant::Classic,
                        legacy,
                        black_parts,
                        own: false,
                    },
                }
            }
            Ok(Err(problem)) => ProjectsSkinPick::Refused {
                reason: pick_refusal(problem),
            },
            Err(_) => ProjectsSkinPick::Refused {
                reason: ProjectsSkinPickRefusal::Unreadable,
            },
        }
    }

    /// A premium player's skin by name; its signed texture stays here for `save`.
    pub async fn lookup(&self, account: &SkinAccount, name: &str) -> ProjectsSkinLookup {
        match self.inner.mojang.lookup(name.trim()).await {
            PremiumLookup::Found(skin) => {
                let Ok(preview) = skin_preview(&skin.png) else {
                    return ProjectsSkinLookup::Unreachable;
                };
                let picture = data_url(&preview);
                let own = skin.name.eq_ignore_ascii_case(&account.name);
                let (shown_name, variant) = (skin.name.clone(), skin.variant);
                let id = self.keep_choice(Choice::Premium {
                    skin,
                    preview: Arc::new(preview),
                });
                ProjectsSkinLookup::Found {
                    choice: ProjectsSkinChoice {
                        id,
                        kind: ProjectsSkinChoiceKind::Premium,
                        picture,
                        name: shown_name,
                        variant,
                        legacy: false,
                        black_parts: false,
                        own,
                    },
                }
            }
            PremiumLookup::NoAccount => ProjectsSkinLookup::NoAccount,
            PremiumLookup::NoSkin => ProjectsSkinLookup::NoSkin,
            PremiumLookup::Unreachable => ProjectsSkinLookup::Unreachable,
        }
    }

    // --- what the interface sends -------------------------------------------------------------

    fn prepare(&self, account: &SkinAccount, save: ProjectsSkinSave) -> Option<Prepared> {
        match save {
            ProjectsSkinSave::Choice { id, variant } => {
                let choice = self
                    .lock()
                    .choices
                    .iter()
                    .find(|(kept, _)| *kept == id)
                    .map(|(_, choice)| choice.clone())?;
                Some(match choice {
                    Choice::Png(skin) => {
                        // Old 64×32 skins have no slim arms.
                        let variant = if skin.legacy {
                            SkinVariant::Classic
                        } else {
                            variant
                        };
                        Prepared {
                            source: McSource::Png {
                                variant,
                                png: skin.png.clone(),
                                sha256: skin.sha256.clone(),
                            },
                            kept: KeptSource::Png {
                                variant,
                                sha256: skin.sha256.clone(),
                                texture: None,
                            },
                            picture: Some(skin.png.clone()),
                            preview: Some(skin.preview.clone()),
                            variant: Some(variant),
                        }
                    }
                    Choice::Premium { skin, preview } => Prepared {
                        source: McSource::Premium(McPremium {
                            name: skin.name.clone(),
                            id: skin.id.clone(),
                            texture: skin.texture.clone(),
                            variant: skin.variant,
                            value: skin.value.clone(),
                            signature: skin.signature.clone(),
                        }),
                        kept: KeptSource::Premium {
                            name: skin.name.clone(),
                            id: skin.id.clone(),
                            texture: skin.texture.clone(),
                            variant: skin.variant,
                            value: skin.value.clone(),
                            signature: skin.signature.clone(),
                        },
                        picture: Some(skin.png.clone()),
                        preview: Some(preview.as_ref().clone()),
                        variant: Some(skin.variant),
                    },
                })
            }
            ProjectsSkinSave::Default => Some(Prepared {
                source: McSource::Default,
                kept: KeptSource::Default,
                picture: None,
                preview: None,
                variant: None,
            }),
            ProjectsSkinSave::Undo => {
                let (previous, picture) = self.inner.store.previous(&account.id, &account.name)?;
                let source = previous.to_source(picture.clone())?;
                let preview = picture
                    .as_deref()
                    .and_then(|picture| skin_preview(picture).ok());
                Some(Prepared {
                    source,
                    variant: previous.variant(),
                    kept: previous,
                    picture,
                    preview,
                })
            }
        }
    }

    /// Saves a picked skin, the default or the skin before the last change ("Geri al") as
    /// `account`'s Projects skin.
    pub async fn save(&self, account: &SkinAccount, save: ProjectsSkinSave) -> ProjectsSkinOutcome {
        let Some((session, generation)) = self.session() else {
            return ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::Session);
        };
        let Some(prepared) = self.prepare(account, save) else {
            return ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::ChoiceGone);
        };
        match session.mc_put_skin(&account.name, &prepared.source).await {
            Ok(job) => {
                if let Err(error) = self.inner.store.set_pending(
                    &account.id,
                    &account.name,
                    &job.id,
                    &prepared.kept,
                    prepared.picture.as_deref(),
                ) {
                    tracing::info!(%error, "the Projects skin change is not kept on this computer");
                }
                ProjectsSkinOutcome::Sent {
                    job: Box::new(self.sent(
                        generation,
                        job,
                        McJobKind::Skin,
                        Some(account.clone()),
                        prepared.preview.as_deref().map(data_url),
                        prepared.variant,
                    )),
                }
            }
            Err(failure) => {
                self.failed(generation, &failure);
                ProjectsSkinOutcome::Refused {
                    refusal: refusal_of(&failure),
                }
            }
        }
    }

    /// Cancels a job ProBot has not taken yet; false once it has.
    pub async fn cancel(&self, job: &str) -> Result<bool, Box<McFailure>> {
        let Some((session, generation)) = self.session() else {
            return Err(Box::new(McFailure::signed_out()));
        };
        match session.mc_cancel_job(job).await {
            // Gone already: nothing is left to cancel.
            Ok(()) => {}
            Err(failure) if failure.status == 404 => {}
            Err(failure) if failure.status == 409 && failure.code == "job_taken" => {
                return Ok(false);
            }
            Err(failure) => {
                self.failed(generation, &failure);
                return Err(failure);
            }
        }
        let sent = {
            let mut state = self.lock();
            if let Some(known) = &mut state.known {
                known.jobs.retain(|known| known.id != job);
            }
            state.sent.remove(job)
        };
        if let Some(account) = sent.and_then(|sent| sent.account) {
            self.inner
                .store
                .drop_pending(&account.id, &account.name, job);
        }
        self.emit(ProjectsSkinChange::State);
        Ok(true)
    }

    /// Links the Discord account to `account`'s name with the code from `/discord eşle`.
    pub async fn link(&self, account: &SkinAccount, pin: &str) -> ProjectsSkinOutcome {
        if pin.len() != 6 || !pin.bytes().all(|b| b.is_ascii_digit()) {
            return ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::InvalidPin);
        }
        let Some((session, generation)) = self.session() else {
            return ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::Session);
        };
        let now = unix_now();
        if let Some(at) = self.lock().link_retry_at.filter(|at| *at > now) {
            // Tries are used up: the service would only say so again.
            return ProjectsSkinOutcome::Refused {
                refusal: ProjectsSkinRefusal {
                    retry_after: u64::try_from(at - now).ok(),
                    ..ProjectsSkinRefusal::of(ProjectsSkinRefusalCode::Tries)
                },
            };
        }
        match session.mc_link(&account.name, pin).await {
            Ok(job) => ProjectsSkinOutcome::Sent {
                job: Box::new(self.sent(
                    generation,
                    job,
                    McJobKind::Link,
                    Some(account.clone()),
                    None,
                    None,
                )),
            },
            Err(failure) => {
                self.failed(generation, &failure);
                ProjectsSkinOutcome::Refused {
                    refusal: refusal_of(&failure),
                }
            }
        }
    }

    /// Removes the link (ProBot's `/eşleme kaldır`); the Projects skin stays.
    pub async fn unlink(&self) -> ProjectsSkinOutcome {
        let Some((session, generation)) = self.session() else {
            return ProjectsSkinOutcome::refused(ProjectsSkinRefusalCode::Session);
        };
        match session.mc_unlink().await {
            Ok(job) => ProjectsSkinOutcome::Sent {
                job: Box::new(self.sent(generation, job, McJobKind::Unlink, None, None, None)),
            },
            Err(failure) => {
                self.failed(generation, &failure);
                ProjectsSkinOutcome::Refused {
                    refusal: refusal_of(&failure),
                }
            }
        }
    }

    /// Asks ProBot for the link; the answer comes as a change (`job` for a check job, `state`
    /// for a link the service learned lately).
    pub async fn link_check(&self) {
        let Some((session, generation)) = self.session() else {
            return;
        };
        match session.mc_check().await {
            Ok(McCheck::Job(job)) => {
                self.sent(generation, job, McJobKind::Check, None, None, None);
            }
            Ok(McCheck::Link(link)) => {
                let known = {
                    let mut state = self.lock();
                    let current = state.generation == Some(generation);
                    match &mut state.known {
                        Some(known) if current => {
                            known.link = link;
                            true
                        }
                        _ => false,
                    }
                };
                if known {
                    self.emit(ProjectsSkinChange::State);
                } else {
                    self.read_in_background(&session, generation).await;
                }
            }
            Err(failure) => self.failed(generation, &failure),
        }
    }

    /// A job the service opened for this launcher: kept until it closes, asked about while the
    /// socket is down.
    fn sent(
        &self,
        generation: u64,
        mut job: McJob,
        kind: McJobKind,
        account: Option<SkinAccount>,
        picture: Option<String>,
        variant: Option<SkinVariant>,
    ) -> ProjectsSkinJob {
        if job.kind == McJobKind::Other {
            job.kind = kind;
        }
        {
            let mut state = self.lock();
            if state.generation == Some(generation) {
                state.sent.insert(
                    job.id.clone(),
                    Sent {
                        account,
                        at: Instant::now(),
                        picture: picture.clone(),
                        variant,
                    },
                );
                if let Some(known) = &mut state.known {
                    known.put_job(job.clone());
                }
            }
        }
        if job.state.is_open() {
            self.watch(generation, job.id.clone());
        } else {
            self.closed(generation, job.clone());
        }
        job_view(&job, picture, variant).unwrap_or_else(|| unreachable_kind(&job))
    }

    // --- what the service tells -----------------------------------------------------------------

    fn on_signal(&self, signal: McSignal) {
        match signal {
            McSignal::Message(message) => self.on_message(&message),
            McSignal::Connected => self.on_connected(),
        }
    }

    fn on_message(&self, message: &McMessage) {
        let Some((_, generation)) = self.session() else {
            return;
        };
        let news = {
            let mut state = self.lock();
            let news = news_of(state.known.as_ref().map(|known| &known.link), message);
            if let Some(known) = &mut state.known {
                known.apply(message);
            }
            news
        };
        match news {
            McNews::Job(job) => self.closed(generation, job),
            McNews::Linked(name) => self.emit(ProjectsSkinChange::Linked { name }),
            McNews::State => self.emit(ProjectsSkinChange::State),
        }
    }

    /// The socket is back: jobs sent from here may have closed meanwhile, and the link may have
    /// changed.
    fn on_connected(&self) {
        let Some((session, generation)) = self.session() else {
            return;
        };
        let read = {
            let state = self.lock();
            !state.sent.is_empty() || (state.shown && state.known.is_some())
        };
        if read {
            let this = self.clone();
            self.inner.runtime.spawn(async move {
                this.read_in_background(&session, generation).await;
            });
        }
    }

    /// Jobs sent from here that a read shows closed are told.
    fn reconcile(&self, generation: u64, read: &McState) {
        let closed: Vec<McJob> = {
            let state = self.lock();
            read.jobs
                .iter()
                .filter(|job| !job.state.is_open() && state.sent.contains_key(&job.id))
                .cloned()
                .collect()
        };
        for job in closed {
            self.closed(generation, job);
        }
    }

    /// A job of the player's closed: this computer's copy follows a change it sent, and the
    /// interface is told once.
    fn closed(&self, generation: u64, job: McJob) {
        if job.state.is_open() {
            return;
        }
        let sent = {
            let mut state = self.lock();
            if state.generation != Some(generation) || state.told.contains(&job.id) {
                return;
            }
            state.told.push_back(job.id.clone());
            while state.told.len() > TOLD_KEPT {
                state.told.pop_front();
            }
            if let Some(known) = &mut state.known {
                known.put_job(job.clone());
            }
            state.sent.remove(&job.id)
        };
        let account = sent.as_ref().and_then(|sent| sent.account.clone());
        let mut undoable = false;
        if job.kind == McJobKind::Skin
            && let Some(account) = &account
        {
            if job.applied() {
                let texture = job.result.as_ref().and_then(|r| r.texture.as_deref());
                match self
                    .inner
                    .store
                    .applied(&account.id, &account.name, &job.id, texture)
                {
                    Ok(applied) => {
                        undoable = applied
                            && self
                                .inner
                                .store
                                .read(&account.id, &account.name)
                                .previous
                                .is_some();
                    }
                    Err(error) => {
                        tracing::info!(%error, "the applied Projects skin is not kept here");
                    }
                }
            } else {
                self.inner
                    .store
                    .drop_pending(&account.id, &account.name, &job.id);
            }
        }
        let account_id = account.map(|account| account.id).or_else(|| {
            let name = job.name.as_deref()?;
            (self.inner.accounts)()
                .into_iter()
                .find(|account| account.name.eq_ignore_ascii_case(name))
                .map(|account| account.id)
        });
        let (picture, variant, wait) = sent.map_or((None, None, Duration::ZERO), |sent| {
            (
                sent.picture,
                sent.variant,
                SETTLE.saturating_sub(sent.at.elapsed()),
            )
        });
        let this = self.clone();
        self.inner.runtime.spawn(async move {
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            // "Geri al" needs a change left today: read after this one counted.
            let undo = undoable && this.changes_left(generation).await;
            let Some(view) = job_view(&job, picture, variant) else {
                return;
            };
            this.emit(ProjectsSkinChange::Job {
                job: Box::new(view),
                account_id,
                undo,
                on_projects: this.inner.social.on_projects(),
            });
        });
    }

    async fn changes_left(&self, generation: u64) -> bool {
        if let Some((session, current)) = self.session()
            && current == generation
            && let Ok(read) = session.mc_state().await
        {
            self.remember(generation, &read);
            return read.limits.skins_left > 0;
        }
        self.lock()
            .known
            .as_ref()
            .is_some_and(|known| known.limits.skins_left > 0)
    }

    /// Asks about a job sent from here while the socket is down: every 3 s for 30 s, then every
    /// 10 s up to 5 minutes. Stops once it was told, or the session changed.
    fn watch(&self, generation: u64, id: String) {
        let this = self.clone();
        self.inner.runtime.spawn(async move {
            let started = Instant::now();
            loop {
                let wait = if started.elapsed() < POLL_FAST_FOR {
                    POLL_FAST
                } else {
                    POLL_SLOW
                };
                if started.elapsed() + wait > POLL_FOR {
                    return;
                }
                tokio::time::sleep(wait).await;
                let waiting = {
                    let state = this.lock();
                    state.generation == Some(generation) && state.sent.contains_key(&id)
                };
                if !waiting {
                    return;
                }
                if this.inner.social.socket_online() {
                    continue;
                }
                let Some((session, current)) = this.inner.social.mc_session() else {
                    return;
                };
                if current != generation {
                    return;
                }
                match session.mc_job(&id).await {
                    Ok(job) if !job.state.is_open() => {
                        this.closed(generation, job);
                        return;
                    }
                    Ok(_) => {}
                    Err(failure) if failure.status == 404 => {
                        this.lock().sent.remove(&id);
                        return;
                    }
                    Err(failure) if failure.unauthorized() => {
                        this.failed(generation, &failure);
                        return;
                    }
                    Err(_) => {}
                }
            }
        });
    }
}

/// A view for a job of a kind this version does not know (never one of those it sends).
fn unreachable_kind(job: &McJob) -> ProjectsSkinJob {
    ProjectsSkinJob {
        id: job.id.clone(),
        kind: ProjectsSkinJobKind::Check,
        state: ProjectsSkinJobState::Pending,
        name: job.name.clone(),
        source: None,
        result: None,
        picture: None,
        variant: None,
        created_at_ms: job.created_at,
        updated_at_ms: job.updated_at,
    }
}

#[cfg(test)]
mod tests;

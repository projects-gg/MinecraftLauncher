//! The whole path through real sockets: [`Overlay`] with a fake friends feature, and a mock mod that
//! connects the way the real one does (port and token from the game's JVM arguments, `hello`,
//! then frames both ways).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pl_social::{
    CallAnswer as SocialAnswer, ChatChangeView, ChatPage, MessageState, PresenceState, SocialError,
    SocialNotice, SocialUpdate, SocialView,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::timeout;

use crate::feed::tests::{chat2_view, message, view};
use crate::protocol::ByeReason;
use crate::{Endpoint, Overlay, OverlayConfig, OverlayStatus, PORT_ARG, SocialActions, TOKEN_ARG};

const WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Calls {
    sent: Vec<(String, String)>,
    replies: Vec<(String, String, i64)>,
    stickers: Vec<(String, String, Option<i64>)>,
    discarded: Vec<String>,
    read: Vec<String>,
    typing: Vec<String>,
    reacts: Vec<(String, i64, String, bool)>,
    unfurls: Vec<(String, String)>,
    dismissed: Vec<String>,
    answers: Vec<(String, SocialAnswer)>,
    refuse: Option<SocialError>,
    refuse_reaction: Option<SocialError>,
    /// SKIN1-O: the service offers codeless linking; the permits asked for (by name); a refusal;
    /// how often the permit was closed.
    codeless: bool,
    permits: Vec<String>,
    refuse_permit: bool,
    closed: u32,
    /// OVL-UX: the friend menu's actions as the service heard them, and a refusal for the next.
    invites: Vec<String>,
    removed: Vec<String>,
    blocks: Vec<String>,
    reports: Vec<(String, pl_social::ReportReason, Option<String>, bool)>,
    refuse_people: Option<SocialError>,
    /// OVL-UX2: the categories saved, crew invites, crewmates asked and requests accepted.
    groups_saved: Vec<(Vec<pl_social::FriendGroup>, Vec<pl_social::GroupMember>)>,
    crew_invites: Vec<(String, String)>,
    crewmates_asked: Vec<String>,
    accepted: Vec<String>,
}

/// A permit as the service makes them: `i` and 32 base64url characters.
const NONCE: &str = "iAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB";

/// The friends feature as the overlay uses it, recording what it was asked.
#[derive(Clone)]
struct FakeSocial {
    view: Arc<Mutex<SocialView>>,
    calls: Arc<Mutex<Calls>>,
}

impl FakeSocial {
    fn new(view: SocialView) -> Self {
        Self {
            view: Arc::new(Mutex::new(view)),
            calls: Arc::new(Mutex::new(Calls::default())),
        }
    }

    fn calls(&self) -> std::sync::MutexGuard<'_, Calls> {
        self.calls.lock().unwrap()
    }
}

impl SocialActions for FakeSocial {
    fn view(&self) -> SocialView {
        self.view.lock().unwrap().clone()
    }

    fn chat_page(&self, conv: &str) -> ChatPage {
        let mut linked = message("i:3", "f-ayse", false, MessageState::Sent);
        linked.text = "bak https://youtu.be/dQw4w9WgXcQ".into();
        linked.at = now_ms();
        ChatPage {
            conv: conv.to_owned(),
            messages: vec![
                message("i:1", "f-ayse", false, MessageState::Sent),
                message("i:2", "me", true, MessageState::Sent),
                linked,
            ],
            more: false,
        }
    }

    fn chat_send(&self, conv: &str, text: &str) -> pl_social::Result<String> {
        let mut calls = self.calls();
        if let Some(error) = calls.refuse.take() {
            return Err(error);
        }
        calls.sent.push((conv.to_owned(), text.to_owned()));
        Ok(format!("n:k{}", calls.sent.len()))
    }

    fn chat_reply(&self, conv: &str, text: &str, reply: i64) -> pl_social::Result<String> {
        let mut calls = self.calls();
        calls
            .replies
            .push((conv.to_owned(), text.to_owned(), reply));
        Ok(format!("n:r{}", calls.replies.len()))
    }

    fn chat_sticker(
        &self,
        conv: &str,
        sticker: &str,
        reply: Option<i64>,
    ) -> pl_social::Result<String> {
        let mut calls = self.calls();
        calls
            .stickers
            .push((conv.to_owned(), sticker.to_owned(), reply));
        Ok(format!("n:s{}", calls.stickers.len()))
    }

    fn chat_discard(&self, key: &str) {
        self.calls().discarded.push(key.to_owned());
    }

    fn chat_read(&self, conv: &str) {
        self.calls().read.push(conv.to_owned());
    }

    fn chat_typing(&self, conv: &str) {
        self.calls().typing.push(conv.to_owned());
    }

    fn chat_react(
        &self,
        conv: &str,
        id: i64,
        emoji: &str,
        on: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls
            .reacts
            .push((conv.to_owned(), id, emoji.to_owned(), on));
        std::future::ready(calls.refuse_reaction.take().map_or(Ok(()), Err))
    }

    fn chat_unfurl(
        &self,
        conv: &str,
        key: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        self.calls().unfurls.push((conv.to_owned(), key.to_owned()));
        std::future::ready(Ok(()))
    }

    fn dismiss_invite(&self, id: &str) {
        self.calls().dismissed.push(id.to_owned());
    }

    fn answer_call(
        &self,
        call: &str,
        answer: SocialAnswer,
    ) -> impl Future<Output = pl_social::Result<bool>> + Send + 'static {
        self.calls().answers.push((call.to_owned(), answer));
        std::future::ready(Ok(true))
    }

    fn codeless_offered(&self) -> bool {
        self.calls().codeless
    }

    fn codeless_permit(
        &self,
        name: &str,
    ) -> impl Future<Output = Option<pl_social::McIntent>> + Send + 'static {
        let mut calls = self.calls();
        calls.permits.push(name.to_owned());
        let intent = (!calls.refuse_permit).then(|| pl_social::McIntent {
            nonce: NONCE.to_owned(),
            expires_at: now_ms() / 1000 + 1_800,
        });
        std::future::ready(intent)
    }

    fn codeless_close(&self) -> impl Future<Output = ()> + Send + 'static {
        self.calls().closed += 1;
        std::future::ready(())
    }

    fn invite(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<pl_social::InviteResult>> + Send + 'static {
        let mut calls = self.calls();
        calls.invites.push(friend.to_owned());
        let answer = match calls.refuse_people.take() {
            Some(error) => Err(error),
            // Can is offline: the invite waits for him.
            None if friend == "f-can" => Ok(pl_social::InviteResult::Queued),
            None => Ok(pl_social::InviteResult::Delivered),
        };
        std::future::ready(answer)
    }

    fn remove_friend(
        &self,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls.removed.push(friend.to_owned());
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }

    fn block(&self, friend: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls.blocks.push(friend.to_owned());
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }

    fn report_player(
        &self,
        friend: &str,
        reason: pl_social::ReportReason,
        note: Option<String>,
        block: bool,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls.reports.push((friend.to_owned(), reason, note, block));
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }

    fn save_groups(
        &self,
        groups: Vec<pl_social::FriendGroup>,
        members: Vec<pl_social::GroupMember>,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls.groups_saved.push((groups, members));
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }

    fn invite_to_team(
        &self,
        team: &str,
        friend: &str,
    ) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls
            .crew_invites
            .push((team.to_owned(), friend.to_owned()));
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }

    fn request_crewmate(
        &self,
        user: &str,
    ) -> impl Future<Output = pl_social::Result<pl_social::SendOutcome>> + Send + 'static {
        let mut calls = self.calls();
        calls.crewmates_asked.push(user.to_owned());
        let answer = calls
            .refuse_people
            .take()
            .map_or(Ok(pl_social::SendOutcome::Requested), Err);
        std::future::ready(answer)
    }

    fn accept(&self, user: &str) -> impl Future<Output = pl_social::Result<()>> + Send + 'static {
        let mut calls = self.calls();
        calls.accepted.push(user.to_owned());
        std::future::ready(calls.refuse_people.take().map_or(Ok(()), Err))
    }
}

/// The mod's side of the bridge.
struct MockMod {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl MockMod {
    /// Reads `-Dprojects.overlay.port` and `-Dprojects.overlay.token` like the mod reads its system
    /// properties, connects and says hello.
    async fn connect(endpoint: &Endpoint) -> Self {
        Self::connect_with(endpoint, &[]).await
    }

    /// [`MockMod::connect`] for a mod with these hello features.
    async fn connect_with(endpoint: &Endpoint, features: &[&str]) -> Self {
        let args = endpoint.jvm_args();
        let value = |prefix: &str| {
            args.iter()
                .find_map(|arg| arg.strip_prefix(prefix))
                .unwrap()
                .to_owned()
        };
        let port: u16 = value(PORT_ARG).parse().unwrap();
        let token = value(TOKEN_ARG);
        let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let (read, writer) = stream.into_split();
        let mut client = Self {
            reader: BufReader::new(read),
            writer,
        };
        client
            .send(serde_json::json!({
                "t": "hello", "proto": 1, "token": token, "mod": "0.2.0", "mc": "26.1.2",
                "lang": "tr_tr", "features": features
            }))
            .await;
        client
    }

    async fn send(&mut self, frame: serde_json::Value) {
        self.writer
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn next(&mut self) -> Option<serde_json::Value> {
        let mut line = String::new();
        let n = timeout(WAIT, self.reader.read_line(&mut line))
            .await
            .expect("no frame in time")
            .unwrap_or(0);
        (n > 0).then(|| serde_json::from_str(&line).unwrap())
    }

    /// The next frame of type `t`, skipping others.
    async fn next_of(&mut self, t: &str) -> serde_json::Value {
        loop {
            let frame = self.next().await.expect("the bridge closed");
            if frame["t"] == t {
                return frame;
            }
        }
    }

    /// SKIN1-O: asks for the codeless linking permit and reads the answer (`link-offer` or
    /// `link-clear`). A `history` asked for right after marks the end: when it comes first,
    /// nothing answered (None).
    async fn want_link(&mut self, name: &str) -> Option<serde_json::Value> {
        // The session hears `link-want` at most every 250 ms.
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.send(serde_json::json!({"t":"link-want","name":name}))
            .await;
        // The permit comes from a task of its own: let it land before the marker is asked for.
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.send(serde_json::json!({"t":"open-conv","conv":"d:f-ayse"}))
            .await;
        loop {
            let frame = self.next().await.expect("the bridge closed");
            match frame["t"].as_str() {
                Some("link-offer" | "link-clear") => {
                    self.next_of("history").await;
                    return Some(frame);
                }
                Some("history") => return None,
                _ => {}
            }
        }
    }
}

fn statuses(overlay: &Overlay) -> Arc<Mutex<Vec<bool>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    overlay.on_status(move |OverlayStatus { connected }| sink.lock().unwrap().push(connected));
    seen
}

async fn eventually(what: &str, check: impl Fn() -> bool) {
    let waited = timeout(WAIT, async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "{what}");
}

#[tokio::test]
async fn a_game_session_from_greeting_to_goodbye() {
    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let seen = statuses(&overlay);
    let endpoint = overlay.start(social.clone(), "2.0.0-test").await.unwrap();
    assert!(overlay.active());

    let mut game = MockMod::connect(&endpoint).await;
    let welcome = game.next().await.unwrap();
    assert_eq!(welcome["t"], "welcome");
    assert_eq!(welcome["proto"], 1);
    assert_eq!(welcome["launcher"], "2.0.0-test");
    assert_eq!(welcome["me"]["name"], "Ece");
    assert_eq!(
        welcome["features"],
        serde_json::json!(["friends", "chat", "calls", "people", "people2"])
    );
    let snapshot = game.next().await.unwrap();
    assert_eq!(snapshot["t"], "snapshot");
    assert_eq!(snapshot["friends"][0]["name"], "Ayşe");
    assert_eq!(snapshot["friends"][0]["where"], "projects");
    assert_eq!(snapshot["convs"][0]["title"], "Ayşe");
    assert_eq!(snapshot["calls"][0]["answers"]["coming"], 1);
    eventually("connected is told", || overlay.connected()).await;

    // A direct message pops in game.
    overlay.on_social(SocialUpdate::Notice(SocialNotice::ChatMessage {
        conv: "d:f-ayse:me".into(),
        message: Box::new(message("i:9", "f-ayse", false, MessageState::Sent)),
        name: Some("Ayşe".into()),
        crew: None,
    }));
    let toast = game.next_of("toast").await;
    assert_eq!(toast["kind"], "message");
    assert_eq!(toast["title"], "Ayşe");
    assert_eq!(toast["body"], "metin i:9");
    assert_eq!(toast["conv"], "d:f-ayse:me");

    // A new view becomes only its differences.
    let mut changed = view();
    changed.friends[1].state = PresenceState::Launcher;
    overlay.on_social(SocialUpdate::View(Box::new(changed)));
    let friend = game.next().await.unwrap();
    assert_eq!(friend["t"], "friend");
    assert_eq!(friend["name"], "Can");
    assert_eq!(friend["where"], "launcher");

    // The panel writes: the message goes through the friends feature, the confirmation comes back.
    game.send(serde_json::json!({"t":"send","cid":"c1","conv":"d:f-ayse:me","text":"geliyorum"}))
        .await;
    eventually("the message was handed on", || {
        !social.calls().sent.is_empty()
    })
    .await;
    assert_eq!(
        social.calls().sent[0],
        ("d:f-ayse:me".to_owned(), "geliyorum".to_owned())
    );
    overlay.on_social(SocialUpdate::Chat(ChatChangeView {
        conv: "d:f-ayse:me".into(),
        upsert: vec![message("n:k1", "me", true, MessageState::Sent)],
        remove: Vec::new(),
        reset: false,
    }));
    let sent = game.next_of("sent").await;
    assert_eq!(sent, serde_json::json!({"t":"sent","cid":"c1","id":"n:k1"}));

    // Opening a conversation: its history, and it counts as read.
    game.send(serde_json::json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    let history = game.next_of("history").await;
    assert_eq!(history["conv"], "d:f-ayse:me");
    assert_eq!(history["msgs"][0]["from"], "Ayşe");
    assert_eq!(history["msgs"][1]["mine"], true);
    assert_eq!(social.calls().read, ["d:f-ayse:me"]);
    // A message arriving in the open conversation is read at once, and pops no toast there.
    overlay.on_social(SocialUpdate::Chat(ChatChangeView {
        conv: "d:f-ayse:me".into(),
        upsert: vec![message("i:10", "f-ayse", false, MessageState::Sent)],
        remove: Vec::new(),
        reset: false,
    }));
    assert_eq!(game.next_of("msg").await["id"], "i:10");
    eventually("read again", || social.calls().read.len() == 2).await;

    // Answering the call and joining a friend.
    game.send(serde_json::json!({"t":"answer","call":"k1","answer":"coming"}))
        .await;
    game.send(serde_json::json!({"t":"join","friend":"f-ayse"}))
        .await;
    eventually("the answer went", || !social.calls().answers.is_empty()).await;
    assert_eq!(
        social.calls().answers[0],
        ("k1".to_owned(), SocialAnswer::Yes)
    );
    eventually("the invite was answered by joining", || {
        social.calls().dismissed == ["inv-1"]
    })
    .await;

    // Turning the overlay off during the game ends the session.
    overlay.configure(OverlayConfig {
        enabled: false,
        ..OverlayConfig::default()
    });
    assert_eq!(
        game.next_of("bye").await,
        serde_json::json!({"t":"bye","reason":"disabled"})
    );
    assert!(game.next().await.is_none());
    assert!(!overlay.active() && !overlay.connected());
    assert_eq!(*seen.lock().unwrap(), [true, false]);
}

#[tokio::test]
async fn closing_for_the_game_says_so_and_a_reconnect_is_greeted_again() {
    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social, "2.0.0-test").await.unwrap();
    let mut first = MockMod::connect(&endpoint).await;
    assert_eq!(first.next().await.unwrap()["t"], "welcome");
    assert_eq!(first.next().await.unwrap()["t"], "snapshot");

    // The mod reconnects (after a hiccup): the new connection gets everything again.
    let mut second = MockMod::connect(&endpoint).await;
    assert_eq!(first.next_of("bye").await["reason"], "replaced");
    assert_eq!(second.next().await.unwrap()["t"], "welcome");
    assert_eq!(second.next().await.unwrap()["t"], "snapshot");

    // "Oyun açılınca launcher: Kapansın".
    overlay.stop(ByeReason::GameStartClose).await;
    assert_eq!(
        second.next().await.unwrap(),
        serde_json::json!({"t":"bye","reason":"game-start-close"})
    );
    assert!(second.next().await.is_none());
    assert!(!overlay.active());
}

#[tokio::test]
async fn sends_that_cannot_go_are_told_at_once() {
    let mut offline = view();
    offline.connection = pl_social::Connection::Reconnecting;
    let social = FakeSocial::new(offline);
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect(&endpoint).await;
    game.next_of("snapshot").await;
    game.send(serde_json::json!({"t":"send","cid":"a","conv":"d:f-ayse:me","text":"selam"}))
        .await;
    assert_eq!(
        game.next_of("send-failed").await,
        serde_json::json!({"t":"send-failed","cid":"a","reason":"offline"})
    );
    assert!(social.calls().sent.is_empty());

    // Back online: a refusal of the friends feature says why; a burst stops at the fifth.
    overlay.on_social(SocialUpdate::View(Box::new(view())));
    social.calls().refuse = Some(SocialError::ChatMuted { until: None });
    game.send(serde_json::json!({"t":"send","cid":"b","conv":"d:f-ayse:me","text":"selam"}))
        .await;
    assert_eq!(game.next_of("send-failed").await["reason"], "muted");
    for n in 0..5 {
        game.send(serde_json::json!({"t":"send","cid":format!("c{n}"),"conv":"d:f-ayse:me","text":"selam"}))
            .await;
    }
    assert_eq!(
        game.next_of("send-failed").await,
        serde_json::json!({"t":"send-failed","cid":"c4","reason":"rate"})
    );
    assert_eq!(social.calls().sent.len(), 4);

    // A message the service refused: the panel hears why, the launcher drops it.
    overlay.on_social(SocialUpdate::Chat(ChatChangeView {
        conv: "d:f-ayse:me".into(),
        upsert: vec![{
            let mut failed = message("n:k1", "me", true, MessageState::Failed);
            failed.failure = Some(pl_social::SendFailure::Phone);
            failed
        }],
        remove: Vec::new(),
        reset: false,
    }));
    assert_eq!(
        game.next_of("send-failed").await,
        serde_json::json!({"t":"send-failed","cid":"c0","reason":"phone"})
    );
    eventually("dropped from the history", || {
        social.calls().discarded == ["n:k1"]
    })
    .await;
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn connect_switches_the_game_only_where_and_when_it_may() {
    use crate::ConnectOutcome;

    let unavailable = |reason: &str| ConnectOutcome::Unavailable {
        reason: reason.to_owned(),
    };
    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    // No game, no overlay.
    assert_eq!(
        overlay.connect_in_game("play.projects.gg").await,
        unavailable("no-overlay")
    );
    let endpoint = overlay.start(social, "t").await.unwrap();

    // An older mod (no `connect` in its hello) cannot.
    let mut old = MockMod::connect(&endpoint).await;
    old.next_of("snapshot").await;
    assert_eq!(
        overlay.connect_in_game("play.projects.gg").await,
        unavailable("unsupported")
    );

    // A mod that can: the game joins Projects once it says so.
    let mut game = MockMod::connect_with(&endpoint, &["connect"]).await;
    game.next_of("snapshot").await;
    let asking = {
        let overlay = overlay.clone();
        tokio::spawn(async move { overlay.connect_in_game("play.projects.gg").await })
    };
    let connect = game.next_of("connect").await;
    assert_eq!(connect["host"], "play.projects.gg");
    assert_eq!(connect["port"], 25565);
    // Another one meanwhile waits its turn: busy.
    assert_eq!(
        overlay.connect_in_game("play.projects.gg").await,
        unavailable("busy")
    );
    game.send(serde_json::json!({"t":"connect-result","id":connect["id"],"ok":true}))
        .await;
    assert_eq!(asking.await.unwrap(), ConnectOutcome::Done);

    // A friend's shared server is a target; one nobody shares is not, and nothing is sent.
    let asking = {
        let overlay = overlay.clone();
        tokio::spawn(async move { overlay.connect_in_game("mc.example.net").await })
    };
    let connect = game.next_of("connect").await;
    assert_eq!(connect["host"], "mc.example.net");
    game.send(
        serde_json::json!({"t":"connect-result","id":connect["id"],"ok":false,"reason":"busy"}),
    )
    .await;
    assert_eq!(asking.await.unwrap(), unavailable("busy"));
    assert_eq!(
        overlay.connect_in_game("evil.example.org:25565").await,
        unavailable("not-allowed")
    );

    // No answer in time: unavailable, and the next one may go.
    let started = std::time::Instant::now();
    assert_eq!(
        overlay.connect_in_game("play.projects.gg").await,
        unavailable("timeout")
    );
    assert!(started.elapsed() >= crate::CONNECT_WAIT);
    game.next_of("connect").await;
    let asking = {
        let overlay = overlay.clone();
        tokio::spawn(async move { overlay.connect_in_game("play.projects.gg").await })
    };
    let connect = game.next_of("connect").await;
    // The game closes before it answers.
    drop(game);
    let _ = connect;
    assert_eq!(asking.await.unwrap(), unavailable("no-overlay"));
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn without_a_game_nothing_listens_and_a_new_start_replaces_the_old_session() {
    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let first = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect(&first).await;
    game.next_of("snapshot").await;
    let second = overlay.start(social, "t").await.unwrap();
    // The first session said goodbye as the launcher would on quitting.
    assert_eq!(game.next_of("bye").await["reason"], "launcher-exit");
    assert_ne!(first.port(), second.port());
    // A late stop for the first game's bridge leaves the second game's session alone.
    overlay
        .stop_for(first.port(), ByeReason::LauncherExit)
        .await;
    assert!(overlay.active());
    overlay
        .stop_for(second.port(), ByeReason::LauncherExit)
        .await;
    assert!(!overlay.active());
    // Stopping again is harmless.
    overlay.stop(ByeReason::LauncherExit).await;
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

#[tokio::test]
async fn chat2_from_the_panel_and_to_it() {
    let social = FakeSocial::new(chat2_view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["connect", "chat2"]).await;
    let welcome = game.next_of("welcome").await;
    assert_eq!(
        welcome["features"],
        serde_json::json!([
            "friends",
            "chat",
            "calls",
            "replies",
            "reactions",
            "stickers",
            "typing",
            "reads",
            "links",
            "people",
            "people2"
        ])
    );
    let snapshot = game.next_of("snapshot").await;
    assert_eq!(snapshot["convs"][0]["seen"], 5);

    // "Yazıyor…" reaches the game only while the panel is open.
    let typing = || {
        SocialUpdate::Typing(pl_social::ChatTypingView {
            conv: "d:f-ayse:me".into(),
            by: "f-ayse".into(),
        })
    };
    overlay.on_social(typing());
    game.send(serde_json::json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    let history = game.next_of("history").await;
    assert_eq!(
        history["msgs"][2]["links"][0]["url"],
        "https://youtu.be/dQw4w9WgXcQ"
    );
    assert_eq!(history["msgs"][2]["sid"], 3);
    // Opening it asks for the card of the link on screen, once.
    eventually("the card was asked for", || {
        social.calls().unfurls == [("d:f-ayse:me".to_owned(), "i:3".to_owned())]
    })
    .await;
    overlay.on_social(typing());
    let frame = game.next_of("typing").await;
    assert_eq!(
        frame,
        serde_json::json!({"t":"typing","conv":"d:f-ayse:me","by":"f-ayse","name":"Ayşe"})
    );
    // The player writes: passed on, a burst only once.
    game.send(serde_json::json!({"t":"typing","conv":"d:f-ayse:me"}))
        .await;
    game.send(serde_json::json!({"t":"typing","conv":"d:f-ayse:me"}))
        .await;
    eventually("typing passed on", || !social.calls().typing.is_empty()).await;

    // A sticker as a reply, and a reply in words.
    game.send(serde_json::json!({
        "t":"send","cid":"s1","conv":"d:f-ayse:me","sticker":"pl1/gg","reply":3
    }))
    .await;
    game.send(serde_json::json!({
        "t":"send","cid":"r1","conv":"d:f-ayse:me","text":"tamam","reply":3
    }))
    .await;
    eventually("both went", || {
        social.calls().stickers.len() == 1 && social.calls().replies.len() == 1
    })
    .await;
    assert_eq!(
        social.calls().stickers[0],
        ("d:f-ayse:me".to_owned(), "pl1/gg".to_owned(), Some(3))
    );
    assert_eq!(
        social.calls().replies[0],
        ("d:f-ayse:me".to_owned(), "tamam".to_owned(), 3)
    );
    assert_eq!(social.calls().typing.len(), 1);
    // A made-up sticker never reaches the friends feature.
    game.send(serde_json::json!({"t":"send","cid":"s2","conv":"d:f-ayse:me","sticker":"../x"}))
        .await;
    assert_eq!(
        game.next_of("send-failed").await,
        serde_json::json!({"t":"send-failed","cid":"s2","reason":"feature-off"})
    );

    // A reaction goes; a refused one comes back to the panel.
    game.send(serde_json::json!({"t":"react","conv":"d:f-ayse:me","sid":1,"e":"👍","on":true}))
        .await;
    eventually("the reaction went", || social.calls().reacts.len() == 1).await;
    social.calls().refuse_reaction = Some(SocialError::Refused {
        status: 409,
        code: pl_social::ErrorCode::ChatReactionsFull,
        message: String::new(),
        detail: Some("mine".into()),
    });
    game.send(serde_json::json!({"t":"react","conv":"d:f-ayse:me","sid":1,"e":"😂","on":true}))
        .await;
    assert_eq!(
        game.next_of("react-failed").await,
        serde_json::json!({
            "t":"react-failed","conv":"d:f-ayse:me","sid":1,"e":"😂","reason":"full"
        })
    );

    // The panel closes: no more "yazıyor…".
    game.send(serde_json::json!({"t":"panel","open":false}))
        .await;
    // The panel's frame and the launcher's next update race: let the frame land first.
    tokio::time::sleep(Duration::from_millis(100)).await;
    overlay.on_social(typing());
    overlay.on_social(SocialUpdate::Notice(SocialNotice::ChatMessage {
        conv: "d:f-ayse:me".into(),
        message: Box::new({
            let mut sticker = message("i:20", "f-ayse", false, MessageState::Sent);
            sticker.text = String::new();
            sticker.sticker = Some("pl1/hi".into());
            sticker
        }),
        name: Some("Ayşe".into()),
        crew: None,
    }));
    let toast = game.next().await.unwrap();
    assert_eq!(toast["t"], "toast", "no typing frame came before it");
    assert_eq!(toast["body"], "Çıkartma · Selam");
    assert_eq!(toast["sticker"], "pl1/hi");
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn an_older_mod_hears_none_of_chat2s_frames() {
    let social = FakeSocial::new(chat2_view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["connect"]).await;
    game.next_of("snapshot").await;
    game.send(serde_json::json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    game.next_of("history").await;
    overlay.on_social(SocialUpdate::Typing(pl_social::ChatTypingView {
        conv: "d:f-ayse:me".into(),
        by: "f-ayse".into(),
    }));
    // A sticker message reads as words; no card is asked for a mod that cannot show it.
    overlay.on_social(SocialUpdate::Chat(ChatChangeView {
        conv: "d:f-ayse:me".into(),
        upsert: vec![{
            let mut sticker = message("i:21", "f-ayse", false, MessageState::Sent);
            sticker.text = String::new();
            sticker.sticker = Some("pl1/gg".into());
            sticker
        }],
        remove: Vec::new(),
        reset: false,
    }));
    let msg = game.next().await.unwrap();
    assert_eq!(msg["t"], "msg", "no typing frame came before it");
    assert_eq!(msg["text"], "[Çıkartma: GG]");
    assert!(social.calls().unfurls.is_empty());
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn crew_tags_reach_a_mod_that_draws_them_and_follow_their_wearers() {
    use crate::feed::tests::team6_view;

    let social = FakeSocial::new(team6_view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();

    // An older mod: the rows carry `badge` (it skips the field), no card and no line comes.
    let mut old = MockMod::connect_with(&endpoint, &["connect", "chat2"]).await;
    old.next_of("welcome").await;
    let snapshot = old.next().await.unwrap();
    assert_eq!(snapshot["t"], "snapshot", "no cards before it");
    drop(old);

    let mut game = MockMod::connect_with(&endpoint, &["connect", "chat2", "tags"]).await;
    let welcome = game.next_of("welcome").await;
    assert!(
        welcome["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "tags")
    );
    let cards = game.next().await.unwrap();
    assert_eq!(
        (cards["t"].clone(), cards["all"].clone()),
        ("badges".into(), true.into())
    );
    assert_eq!(cards["cards"]["t9"]["tag"], "GK");
    assert_eq!(cards["cards"]["t9"]["since"], "2026-09");
    assert_eq!(cards["cards"]["t1"]["mine"], true);
    let snapshot = game.next().await.unwrap();
    assert_eq!(snapshot["t"], "snapshot");
    let ayse = snapshot["friends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "f-ayse")
        .unwrap()
        .clone();
    assert_eq!(ayse["badge"], "t9");
    let first = game.next_of("note").await;
    assert_eq!(
        (first["name"].clone(), first["mine"].clone()),
        ("Ece".into(), true.into())
    );
    let second = game.next_of("note").await;
    assert_eq!(
        (
            second["conv"].clone(),
            second["kind"].clone(),
            second["name"].clone()
        ),
        ("c:t1".into(), "joined".into(), "Deniz".into())
    );

    // The panel shows the crew's chat: Ayşe's message names her tag.
    game.send(serde_json::json!({"t":"open-conv","conv":"c:t1"}))
        .await;
    let history = game.next_of("history").await;
    assert_eq!(history["msgs"][0]["badge"], "t9");
    assert!(
        history["msgs"][1].get("badge").is_none(),
        "the player's own"
    );

    // Ayşe takes her tag off: her card goes, her row loses it, and the conversation on screen
    // comes again without it.
    let mut off = team6_view();
    off.friends[0].badge = None;
    *social.view.lock().unwrap() = off.clone();
    overlay.on_social(SocialUpdate::View(Box::new(off)));
    let gone = game.next_of("badges").await;
    assert_eq!(gone["all"], false);
    assert!(gone["cards"]["t9"].is_null());
    let row = game.next_of("friend").await;
    assert!(row["id"] == "f-ayse" && row.get("badge").is_none());
    let again = game.next_of("history").await;
    assert_eq!(again["conv"], "c:t1");
    assert!(again["msgs"][0].get("badge").is_none());
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn world_sharing_hears_a_hosting_mod_and_only_it_gets_host_frames() {
    use crate::protocol::{FromGame, ToGame, WorldMark};
    use crate::{ConnectOutcome, HostSignal, WorldsShown};

    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&heard);
    overlay.on_host(move |signal| sink.lock().unwrap().push(signal));
    // Set before the game: kept for the session that starts later. Mert invited the player.
    overlay.set_worlds(WorldsShown {
        marks: [("f-mert".to_owned(), WorldMark { guests: 1, max: 7 })].into(),
        may_host: true,
    });
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();

    // An older mod: its host frames go nowhere and a world connect is not for it.
    let mut old = MockMod::connect_with(&endpoint, &["connect"]).await;
    old.next_of("snapshot").await;
    old.send(serde_json::json!({"t":"host-world","open":true,"folder":"W","mode":"survival"}))
        .await;
    assert_eq!(
        overlay
            .connect_world_in_game(50123, "Arkadaş dünyası: Mert".into())
            .await,
        ConnectOutcome::Unavailable {
            reason: "unsupported".into()
        }
    );
    eventually("the older mod's hello reached world sharing", || {
        heard
            .lock()
            .unwrap()
            .contains(&HostSignal::Connected { hosts: false })
    })
    .await;

    // A mod that hosts: `host` in its welcome, Mert's row marked and joinable.
    let mut game = MockMod::connect_with(&endpoint, &["connect", "host"]).await;
    let welcome = game.next_of("welcome").await;
    assert!(
        welcome["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "host")
    );
    let snapshot = game.next_of("snapshot").await;
    let mert = snapshot["friends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "f-mert")
        .unwrap()
        .clone();
    assert_eq!(mert["world"], serde_json::json!({"guests": 1, "max": 7}));
    assert_eq!(mert["joinable"], true);
    eventually("the hosting mod's hello reached world sharing", || {
        heard
            .lock()
            .unwrap()
            .contains(&HostSignal::Connected { hosts: true })
    })
    .await;
    assert!(
        !heard
            .lock()
            .unwrap()
            .iter()
            .any(|s| matches!(s, HostSignal::Frame(_))),
        "the older mod's frame went nowhere"
    );

    // Its world reaches world sharing; world sharing's frames reach it.
    game.send(
        serde_json::json!({"t":"host-world","open":true,"folder":"Yeni Dünya","mode":"creative","lan":false}),
    )
    .await;
    eventually("the world reached world sharing", || {
        heard.lock().unwrap().iter().any(|s| {
            matches!(s, HostSignal::Frame(FromGame::HostWorld { open: true, folder, .. })
                if folder == "Yeni Dünya")
        })
    })
    .await;
    overlay.host_send(vec![ToGame::HostPublish {
        id: "p1".into(),
        mode: "creative".into(),
        commands: false,
        max: 7,
        binding: "sW9hzU3c".into(),
    }]);
    let publish = game.next_of("host-publish").await;
    assert_eq!(
        (publish["id"].clone(), publish["max"].clone()),
        ("p1".into(), 7.into())
    );

    // "Katıl" in game on Mert's world goes to world sharing, not to the friends feature.
    game.send(serde_json::json!({"t":"join","friend":"f-mert"}))
        .await;
    eventually("the in-game join reached world sharing", || {
        heard.lock().unwrap().contains(&HostSignal::Join {
            friend: "f-mert".into(),
        })
    })
    .await;
    assert!(social.calls().dismissed.is_empty());

    // Into Mert's world in game: 127.0.0.1 and the label, nothing else.
    let asking = {
        let overlay = overlay.clone();
        tokio::spawn(async move {
            overlay
                .connect_world_in_game(50123, "Arkadaş dünyası: Mert".into())
                .await
        })
    };
    let connect = game.next_of("connect").await;
    assert_eq!(connect["host"], "127.0.0.1");
    assert_eq!(connect["port"], 50123);
    assert_eq!(connect["label"], "Arkadaş dünyası: Mert");
    game.send(serde_json::json!({"t":"connect-result","id":connect["id"],"ok":true}))
        .await;
    assert_eq!(asking.await.unwrap(), ConnectOutcome::Done);

    // The game goes: world sharing hears it.
    drop(game);
    eventually("world sharing heard the mod go", || {
        heard.lock().unwrap().contains(&HostSignal::Gone)
    })
    .await;
    overlay.stop(ByeReason::LauncherExit).await;
}

fn lists(welcome: &serde_json::Value, feature: &str) -> bool {
    welcome["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == feature)
}

#[tokio::test]
async fn codeless_linking_vouches_only_for_the_games_own_account() {
    let social = FakeSocial::new(view());
    social.calls().codeless = true;
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay
        .start_with_game(social.clone(), "t", "Ahmet")
        .await
        .unwrap();

    // An older mod asks nothing and hears nothing of it.
    let mut old = MockMod::connect_with(&endpoint, &["connect", "chat2"]).await;
    assert!(lists(&old.next_of("welcome").await, "link"));
    assert_eq!(old.want_link("Ahmet").await, None);
    drop(old);

    let mut game = MockMod::connect_with(&endpoint, &["connect", "chat2", "link"]).await;
    assert!(lists(&game.next_of("welcome").await, "link"));
    // Another account than the one the game plays as: no permit, the service is not asked.
    let other = game.want_link("Mehmet").await.unwrap();
    assert_eq!(other["t"], "link-clear");
    assert!(social.calls().permits.is_empty());
    // The game's own account (whatever its case): the permit, for the launcher's spelling.
    let offer = game.want_link("ahmet").await.unwrap();
    assert_eq!(offer["t"], "link-offer");
    assert_eq!(offer["nonce"], NONCE);
    assert_eq!(offer["name"], "Ahmet");
    assert!(offer["expiresAt"].as_i64().unwrap() > now_ms() / 1000 + 1_700);
    // Asked again (`/discord eşle` later): the same permit, the service is not asked again.
    let again = game.want_link("Ahmet").await.unwrap();
    assert_eq!(again["nonce"], NONCE);
    assert_eq!(social.calls().permits, ["Ahmet"]);

    // Another friends account signs in: the permit is worthless, the mod forgets it.
    let mut other_account = view();
    let me = other_account.me.as_mut().unwrap();
    me.id = "me-2".into();
    me.name = "Deniz".into();
    *social.view.lock().unwrap() = other_account.clone();
    overlay.on_social(SocialUpdate::View(Box::new(other_account)));
    game.next_of("link-clear").await;

    // The game ends: the permit it opened stops working.
    overlay.stop(ByeReason::LauncherExit).await;
    eventually("the permit was closed", || social.calls().closed == 1).await;
}

#[tokio::test]
async fn without_codeless_linking_the_mod_hears_there_is_no_permit() {
    // Studio has it off (or the player is not signed in to friends): not in the welcome, and a
    // server's question gets "none" at once.
    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay
        .start_with_game(social.clone(), "t", "Ahmet")
        .await
        .unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["link"]).await;
    assert!(!lists(&game.next_of("welcome").await, "link"));
    assert_eq!(game.want_link("Ahmet").await.unwrap()["t"], "link-clear");

    // Switched on: the welcome comes again with it.
    social.calls().codeless = true;
    overlay.on_social(SocialUpdate::View(Box::new(view())));
    assert!(lists(&game.next_of("welcome").await, "link"));

    // The service refuses (linked, said "Hayır" lately, paused): none either.
    social.calls().refuse_permit = true;
    assert_eq!(game.want_link("Ahmet").await.unwrap()["t"], "link-clear");
    assert_eq!(social.calls().permits, ["Ahmet"]);
    overlay.stop(ByeReason::LauncherExit).await;
    // A refused permit was asked for, so the session closes what it may have opened.
    eventually("closed after the game", || social.calls().closed == 1).await;

    // A game this launcher did not name an account for vouches for nobody.
    let social = FakeSocial::new(view());
    social.calls().codeless = true;
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["link"]).await;
    assert!(!lists(&game.next_of("welcome").await, "link"));
    assert_eq!(game.want_link("Ahmet").await.unwrap()["t"], "link-clear");
    assert!(social.calls().permits.is_empty());
    overlay.stop(ByeReason::LauncherExit).await;
    assert_eq!(social.calls().closed, 0);
}

/// A friend's row in a snapshot.
fn friend_row(snapshot: &serde_json::Value, id: &str) -> serde_json::Value {
    snapshot["friends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id)
        .unwrap()
        .clone()
}

#[tokio::test]
async fn the_friend_menu_invites_and_opens_discord_and_says_how_it_went() {
    use serde_json::json;

    let mut start = view();
    // Ayşe's Discord profile is known; the player is on Projects ("Birlikte oyna" can go).
    start.friends[0].discord_url = "https://discord.com/users/f-ayse".into();
    start.activity = pl_social::ActivityView::Playing {
        host: Some("play.projects.gg".into()),
    };
    let social = FakeSocial::new(start);
    let overlay = Overlay::new(OverlayConfig::default());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&opened);
    overlay.on_discord(move |id, url| sink.lock().unwrap().push((id, url)));
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();

    // An older mod: the rows carry the new fields (it skips them); a press of its is carried out,
    // never answered.
    let mut old = MockMod::connect_with(&endpoint, &["connect", "chat2"]).await;
    let welcome = old.next_of("welcome").await;
    assert!(lists(&welcome, "people") && lists(&welcome, "invite"));
    let snapshot = old.next_of("snapshot").await;
    let ayse = friend_row(&snapshot, "f-ayse");
    assert_eq!(ayse["dm"], "d:me:f-ayse");
    assert_eq!(ayse["discord"], true);
    assert!(friend_row(&snapshot, "f-can").get("discord").is_none());
    old.send(json!({"t":"invite","friend":"f-ayse"})).await;
    eventually("the older mod's invite went", || {
        social.calls().invites.len() == 1
    })
    .await;
    old.send(json!({"t":"open-conv","conv":"d:me:f-ayse"}))
        .await;
    assert_eq!(old.next().await.unwrap()["t"], "history", "no answer first");
    drop(old);

    let mut game = MockMod::connect_with(&endpoint, &["connect", "chat2", "people"]).await;
    game.next_of("snapshot").await;
    // Delivered to Ayşe, waiting for Can (offline).
    game.send(json!({"t":"invite","friend":"f-ayse"})).await;
    assert_eq!(
        game.next_of("friend-result").await,
        json!({"t":"friend-result","friend":"f-ayse","action":"invite","ok":true})
    );
    game.send(json!({"t":"invite","friend":"f-can"})).await;
    assert_eq!(game.next_of("friend-result").await["queued"], true);
    // The service says it is too soon.
    social.calls().refuse_people = Some(SocialError::RateLimited {
        retry_after: None,
        item: None,
    });
    game.send(json!({"t":"invite","friend":"f-mert"})).await;
    let wait = game.next_of("friend-result").await;
    assert_eq!(
        (wait["ok"].clone(), wait["reason"].clone()),
        (false.into(), "wait".into())
    );
    // Someone not on the friend list: refused at once, the service never asked.
    game.send(json!({"t":"block","friend":"x-bad"})).await;
    assert_eq!(game.next_of("friend-result").await["reason"], "failed");
    assert!(social.calls().blocks.is_empty());
    // Ayşe's profile opens through the launcher, once for a double press; Can has none.
    game.send(json!({"t":"discord","friend":"f-ayse"})).await;
    game.send(json!({"t":"discord","friend":"f-ayse"})).await;
    game.send(json!({"t":"discord","friend":"f-can"})).await;
    let none = game.next_of("friend-result").await;
    assert_eq!(
        (
            none["friend"].clone(),
            none["action"].clone(),
            none["reason"].clone()
        ),
        ("f-can".into(), "discord".into(), "failed".into())
    );
    assert_eq!(
        *opened.lock().unwrap(),
        [(
            "f-ayse".to_owned(),
            "https://discord.com/users/f-ayse".to_owned()
        )]
    );
    assert_eq!(
        social.calls().invites,
        ["f-ayse", "f-ayse", "f-can", "f-mert"]
    );
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn the_friend_menu_unfriends_blocks_reports_and_holds_back_a_flood() {
    use serde_json::json;

    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["people"]).await;
    let welcome = game.next_of("welcome").await;
    assert!(lists(&welcome, "people"));
    assert!(
        !lists(&welcome, "invite"),
        "in the launcher: no Birlikte oyna"
    );
    game.next_of("snapshot").await;

    game.send(json!({"t":"unfriend","friend":"f-mert"})).await;
    assert_eq!(
        game.next_of("friend-result").await,
        json!({"t":"friend-result","friend":"f-mert","action":"unfriend","ok":true})
    );
    game.send(json!({"t":"report","friend":"f-zey","reason":"harassment","note":"  sürekli\n küfür ","block":true}))
        .await;
    assert_eq!(game.next_of("friend-result").await["action"], "report");
    assert_eq!(
        social.calls().reports,
        [(
            "f-zey".to_owned(),
            pl_social::ReportReason::Harassment,
            Some("sürekli küfür".to_owned()),
            true
        )]
    );
    // A message's reason is not a player's.
    game.send(json!({"t":"report","friend":"f-zey","reason":"inappropriate"}))
        .await;
    assert_eq!(game.next_of("friend-result").await["reason"], "failed");
    // The friends service is away: the block does not go.
    social.calls().refuse_people = Some(SocialError::Unavailable { retry_after: None });
    game.send(json!({"t":"block","friend":"f-can"})).await;
    let away = game.next_of("friend-result").await;
    assert_eq!(
        (
            away["action"].clone(),
            away["ok"].clone(),
            away["reason"].clone()
        ),
        ("block".into(), false.into(), "offline".into())
    );
    // "Birlikte oyna" cannot go now: refused without asking the service.
    game.send(json!({"t":"invite","friend":"f-ayse"})).await;
    assert_eq!(game.next_of("friend-result").await["reason"], "failed");
    assert!(social.calls().invites.is_empty());
    // Four went through the gate so far; two more go, the next is held back.
    game.send(json!({"t":"block","friend":"f-ayse"})).await;
    assert_eq!(game.next_of("friend-result").await["ok"], true);
    game.send(json!({"t":"block","friend":"f-zey"})).await;
    assert_eq!(game.next_of("friend-result").await["ok"], true);
    game.send(json!({"t":"block","friend":"f-can"})).await;
    assert_eq!(game.next_of("friend-result").await["reason"], "rate");
    assert_eq!(social.calls().blocks, ["f-can", "f-ayse", "f-zey"]);
    assert_eq!(social.calls().removed, ["f-mert"]);
    overlay.stop(ByeReason::LauncherExit).await;
}

/// OVL-UX2: the player's categories (Can in "Okul"), Mert invited to the crew already, crewmates
/// allowed to ask each other, and Cem (a crewmate, not a friend) asking the player.
fn people2_view() -> SocialView {
    let mut v = view();
    v.groups = Some(pl_social::FriendGroupsView {
        version: 3,
        groups: vec![
            pl_social::FriendGroup {
                id: "g1".into(),
                name: "Okul".into(),
            },
            pl_social::FriendGroup {
                id: "g2".into(),
                name: "PvP".into(),
            },
        ],
        members: vec![pl_social::GroupMember {
            friend: "f-can".into(),
            group: "g1".into(),
        }],
    });
    v.crews[0].invited = vec!["f-mert".into()];
    v.team_features.push("crewFriendRequests".into());
    v.incoming = vec![pl_social::RequestView {
        id: "u-cem".into(),
        name: "Cem".into(),
        username: "cem".into(),
        avatar: "discord".into(),
        avatar_url: None,
        mark: None,
        sent_at: 1,
    }];
    v
}

#[tokio::test]
async fn favourites_online_notices_and_categories_from_the_game() {
    use serde_json::json;

    let social = FakeSocial::new(people2_view());
    let overlay = Overlay::new(OverlayConfig::default());
    let changes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&changes);
    overlay.on_people(move |change| sink.lock().unwrap().push(change));
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();

    // An older mod (OVL-UX's menu only): no lists, and a favourite from it changes nothing.
    let mut old = MockMod::connect_with(&endpoint, &["people"]).await;
    let welcome = old.next_of("welcome").await;
    assert!(lists(&welcome, "people2"));
    assert_eq!(old.next().await.unwrap()["t"], "snapshot", "no lists first");
    old.send(json!({"t":"favorite","friend":"f-ayse","on":true}))
        .await;
    old.send(json!({"t":"open-conv","conv":"d:me:f-ayse"}))
        .await;
    assert_eq!(old.next().await.unwrap()["t"], "history", "nothing first");
    assert!(changes.lock().unwrap().is_empty());
    drop(old);

    let mut game = MockMod::connect_with(&endpoint, &["people", "people2"]).await;
    game.next_of("welcome").await;
    let lists_frame = game.next().await.unwrap();
    assert_eq!(
        lists_frame["t"], "lists",
        "the lists before the friends that name them"
    );
    assert_eq!(
        lists_frame["groups"],
        json!([{"id":"g1","name":"Okul"},{"id":"g2","name":"PvP"}])
    );
    assert_eq!(
        lists_frame["crews"],
        json!([{"id":"t1","name":"Gece Kuşları"}])
    );
    assert_eq!(lists_frame["crewmates"], true);
    assert_eq!(lists_frame["asked"], json!(["u-cem"]));
    let snapshot = game.next_of("snapshot").await;
    assert_eq!(friend_row(&snapshot, "f-can")["group"], "g1");
    assert_eq!(
        friend_row(&snapshot, "f-mert")["crews"],
        json!({"t1":"invited"})
    );
    assert!(friend_row(&snapshot, "f-ayse").get("fav").is_none());

    // A favourite: the friend's frame says so at once (favourites are told when they come
    // online), the choices here follow and the page hears it.
    game.send(json!({"t":"favorite","friend":"f-ayse","on":true}))
        .await;
    let ayse = game.next_of("friend").await;
    assert_eq!(ayse["id"], "f-ayse");
    assert_eq!(
        (ayse["fav"].clone(), ayse["alert"].clone()),
        (true.into(), true.into())
    );
    assert_eq!(
        changes.lock().unwrap().as_slice(),
        [crate::PeopleChange::Favorite {
            friend: "f-ayse".into(),
            on: true
        }]
    );
    assert!(overlay.config().notify.favorite("f-ayse"));
    // "Çevrimiçi olunca bildir" off for her alone; the page pushing the same choices back changes
    // nothing.
    game.send(json!({"t":"alert","friend":"f-ayse","on":false}))
        .await;
    let ayse = game.next_of("friend").await;
    assert!(ayse.get("alert").is_none() && ayse["fav"] == true);
    assert!(!overlay.configure(overlay.config()));

    // A category: the document saved whole, as the launcher's list saves it.
    game.send(json!({"t":"move","friend":"f-ayse","group":"g2"}))
        .await;
    assert_eq!(
        game.next_of("friend-result").await,
        json!({"t":"friend-result","friend":"f-ayse","action":"move","ok":true})
    );
    let (groups, members) = social.calls().groups_saved[0].clone();
    assert_eq!(groups.len(), 2);
    assert!(
        members
            .iter()
            .any(|m| m.friend == "f-ayse" && m.group == "g2")
    );
    assert!(
        members
            .iter()
            .any(|m| m.friend == "f-can" && m.group == "g1")
    );
    // Back to "Arkadaşlar"; a category that does not exist goes nowhere.
    game.send(json!({"t":"move","friend":"f-can","group":""}))
        .await;
    assert_eq!(game.next_of("friend-result").await["ok"], true);
    assert!(
        !social.calls().groups_saved[1]
            .1
            .iter()
            .any(|m| m.friend == "f-can")
    );
    game.send(json!({"t":"move","friend":"f-can","group":"nope"}))
        .await;
    assert_eq!(game.next_of("friend-result").await["ok"], false);
    assert_eq!(social.calls().groups_saved.len(), 2);
    // The category they are in already (the service's view still has Can in "Okul"): nothing to
    // save, and no failure to show.
    game.send(json!({"t":"move","friend":"f-can","group":"g1"}))
        .await;
    assert_eq!(game.next_of("friend-result").await["ok"], true);
    assert_eq!(social.calls().groups_saved.len(), 2);
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn crew_invites_and_crewmates_from_the_game_and_never_anyone_else() {
    use serde_json::json;

    let social = FakeSocial::new(people2_view());
    let overlay = Overlay::new(OverlayConfig::default());
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["people", "people2"]).await;
    game.next_of("snapshot").await;

    // "Ekibe davet et": Ayşe may be invited; Mert is invited already.
    game.send(json!({"t":"crew-invite","friend":"f-ayse","crew":"t1"}))
        .await;
    assert_eq!(
        game.next_of("friend-result").await,
        json!({"t":"friend-result","friend":"f-ayse","action":"crew-invite","ok":true})
    );
    game.send(json!({"t":"crew-invite","friend":"f-mert","crew":"t1"}))
        .await;
    assert_eq!(game.next_of("friend-result").await["ok"], false);
    assert_eq!(
        social.calls().crew_invites,
        [("t1".to_owned(), "f-ayse".to_owned())]
    );

    // Cem asked first: "İsteği kabul et" makes them friends (not queued).
    game.send(json!({"t":"add-friend","friend":"u-cem"})).await;
    let added = game.next_of("friend-result").await;
    assert_eq!(
        (
            added["action"].clone(),
            added["ok"].clone(),
            added.get("queued").is_none()
        ),
        ("add-friend".into(), true.into(), true)
    );
    assert_eq!(social.calls().accepted, ["u-cem"]);
    // A friend is no crewmate to ask.
    game.send(json!({"t":"add-friend","friend":"f-ayse"})).await;
    assert_eq!(game.next_of("friend-result").await["ok"], false);
    assert!(social.calls().crewmates_asked.is_empty());

    // Crew chat's sender menu: a crewmate may be blocked; someone the player shares nothing with
    // may not be reported from the game.
    game.send(json!({"t":"block","friend":"u-cem"})).await;
    assert_eq!(game.next_of("friend-result").await["ok"], true);
    game.send(json!({"t":"report","friend":"u-zz","reason":"spam"}))
        .await;
    assert_eq!(game.next_of("friend-result").await["ok"], false);
    assert_eq!(social.calls().blocks, ["u-cem"]);
    assert!(social.calls().reports.is_empty());
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn a_crewmates_katil_and_discord_reach_crew_chats_menu() {
    use serde_json::json;

    // Cem (a crewmate, not a friend) plays on Projects and his Discord profile is known.
    let mut start = people2_view();
    let cem = &mut start.crews[0].members[1];
    assert_eq!(cem.id, "u-cem");
    cem.state = PresenceState::Playing;
    cem.game = crate::feed::tests::projects();
    cem.discord_url = "https://discord.com/users/u-cem".into();
    let social = FakeSocial::new(start);
    let overlay = Overlay::new(OverlayConfig::default());
    let opened = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&opened);
    overlay.on_discord(move |id, url| sink.lock().unwrap().push((id, url)));
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    let mut game = MockMod::connect_with(&endpoint, &["people", "people2"]).await;
    let lists_frame = game.next_of("lists").await;
    assert_eq!(lists_frame["projects"], json!(["u-cem"]));
    assert_eq!(lists_frame["discord"], json!(["u-cem"]));
    game.next_of("snapshot").await;

    // His menu's "Discord'dan mesaj at" opens his profile through the launcher.
    game.send(json!({"t":"discord","friend":"u-cem"})).await;
    eventually("the crewmate's profile opened", || {
        opened.lock().unwrap().len() == 1
    })
    .await;
    assert_eq!(
        *opened.lock().unwrap(),
        [(
            "u-cem".to_owned(),
            "https://discord.com/users/u-cem".to_owned()
        )]
    );
    // Someone the player shares nothing with: never.
    game.send(json!({"t":"discord","friend":"u-zz"})).await;
    assert_eq!(game.next_of("friend-result").await["reason"], "failed");
    assert_eq!(opened.lock().unwrap().len(), 1);
    overlay.stop(ByeReason::LauncherExit).await;
}

/// OVL-LANG: the overlay speaks the launcher's language until the player picks one in its settings;
/// the pick holds for this game and the next, and everything the launcher words for the mod
/// follows it. An older mod keeps its game's language and cannot pick.
#[tokio::test]
async fn the_overlay_speaks_the_launchers_language_until_the_player_picks_one() {
    use pl_lang::Lang;
    use serde_json::json;

    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    overlay.set_launcher_language(Lang::En);
    let picks = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&picks);
    overlay.on_language(move |lang| sink.lock().unwrap().push(lang));
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    // G2: the game starts in it.
    assert!(
        endpoint
            .jvm_args()
            .contains(&format!("{}en", crate::LANG_ARG))
    );

    // An older mod: its welcome names no language, the launcher words for it in its game's
    // language, and a pick from it changes nothing.
    let mut old = MockMod::connect_with(&endpoint, &["chat2"]).await;
    assert!(old.next_of("welcome").await.get("lang").is_none());
    old.send(json!({"t":"lang","lang":"en"})).await;
    old.send(json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    assert_eq!(old.next_of("history").await["t"], "history");
    assert!(picks.lock().unwrap().is_empty());
    drop(old);

    // G3: a mod whose words follow the launcher hears the overlay's language, here the launcher's.
    let mut game = MockMod::connect_with(&endpoint, &["chat2", "lang"]).await;
    assert_eq!(game.next_of("welcome").await["lang"], "en");
    game.next_of("snapshot").await;
    let sticker = |key: &str| {
        let mut m = message(key, "f-ayse", false, MessageState::Sent);
        m.sticker = Some("pl1/gg".into());
        SocialUpdate::Notice(SocialNotice::ChatMessage {
            conv: "d:f-ayse:me".into(),
            message: Box::new(m),
            name: Some("Ayşe".into()),
            crew: None,
        })
    };
    overlay.on_social(sticker("i:21"));
    assert!(
        game.next_of("toast").await["body"]
            .as_str()
            .unwrap()
            .starts_with("Sticker")
    );

    // G4: the player picks Turkish in the game: a new greeting in it, the launcher keeps it.
    game.send(json!({"t":"lang","lang":"tr"})).await;
    let welcome = game.next_of("welcome").await;
    assert_eq!(welcome["lang"], "tr");
    game.next_of("snapshot").await;
    assert_eq!(picks.lock().unwrap().as_slice(), [Lang::Tr]);
    assert_eq!(overlay.language_pick(), Some(Lang::Tr));
    overlay.on_social(sticker("i:22"));
    assert!(
        game.next_of("toast").await["body"]
            .as_str()
            .unwrap()
            .starts_with("Çıkartma")
    );
    // The same pick again, a value that is no language, and the launcher changing its own
    // language: the pick holds, nothing greets again.
    game.send(json!({"t":"lang","lang":"tr"})).await;
    game.send(json!({"t":"lang","lang":"de"})).await;
    overlay.set_launcher_language(Lang::Tr);
    overlay.set_launcher_language(Lang::En);
    game.send(json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    assert_eq!(
        game.next().await.unwrap()["t"],
        "history",
        "no greeting first"
    );
    assert_eq!(
        picks.lock().unwrap().len(),
        2,
        "the same pick is kept again"
    );

    // Without a pick the overlay follows the launcher again, at once.
    overlay.set_language_pick(None);
    assert_eq!(game.next_of("welcome").await["lang"], "en");
    overlay.set_launcher_language(Lang::Tr);
    assert_eq!(game.next_of("welcome").await["lang"], "tr");
    overlay.stop(ByeReason::LauncherExit).await;

    // The next game starts in the language that holds now.
    overlay.set_language_pick(Some(Lang::En));
    let next = overlay.start(social, "t").await.unwrap();
    assert!(next.jvm_args().contains(&format!("{}en", crate::LANG_ARG)));
    overlay.stop(ByeReason::LauncherExit).await;
}

#[tokio::test]
async fn the_emoji_picker_hears_its_switch_and_the_servers_list() {
    use serde_json::json;

    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig::default());
    let seen = statuses(&overlay);
    let switched = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&switched);
    overlay.on_emoji(move |on| sink.lock().unwrap().push(on));
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    // Both switches on: neither is named on the command line.
    assert!(
        endpoint
            .jvm_args()
            .iter()
            .all(|arg| !arg.starts_with(crate::PANEL_ARG) && !arg.starts_with(crate::EMOJI_ARG))
    );

    // An older mod hears nothing of the picker: the answer to its first question comes first.
    let mut old = MockMod::connect_with(&endpoint, &["chat2"]).await;
    old.next_of("snapshot").await;
    old.send(json!({"t":"open-conv","conv":"d:f-ayse:me"}))
        .await;
    assert_eq!(old.next().await.unwrap()["t"], "history");
    old.send(json!({"t":"emoji","on":false})).await;
    drop(old);

    // E3: after the greeting, the switch and the built-in list.
    let mut game = MockMod::connect_with(&endpoint, &["chat2", "lang", "emoji"]).await;
    game.next_of("snapshot").await;
    let emojis = game.next_of("emojis").await;
    assert_eq!(emojis["on"], true);
    assert_eq!(emojis["hosts"], json!(["projects.gg"]));
    let builtin = pl_content::ChatEmojis::builtin();
    assert_eq!(
        emojis["emojis"].as_array().unwrap().len(),
        builtin.emojis.len()
    );
    assert!(
        switched.lock().unwrap().is_empty(),
        "the older mod changed nothing"
    );

    // Studio's list arrives with a content refresh: the game hears it once.
    let mut published = builtin.clone();
    published.emojis.truncate(2);
    overlay.set_emojis(published.clone());
    overlay.set_emojis(published);
    let emojis = game.next_of("emojis").await;
    assert_eq!(emojis["emojis"].as_array().unwrap().len(), 2);
    assert_eq!(emojis["emojis"][0]["code"], ":kalp:");

    // E4: switched off in the game: the launcher keeps it, the game hears it back.
    game.send(json!({"t":"emoji","on":false})).await;
    assert_eq!(game.next_of("emojis").await["on"], false);
    assert_eq!(switched.lock().unwrap().as_slice(), [false]);
    assert!(!overlay.emoji_enabled());
    // The page's own switch reaches the game the same way.
    overlay.configure(OverlayConfig::default());
    assert_eq!(game.next_of("emojis").await["on"], true);
    assert_eq!(
        switched.lock().unwrap().len(),
        1,
        "the page's push is not an echo"
    );
    overlay.stop(ByeReason::LauncherExit).await;
    assert_eq!(*seen.lock().unwrap(), [true, false, true, false]);
}

#[tokio::test]
async fn a_game_with_the_emoji_picker_alone_hears_only_its_words_and_list() {
    use serde_json::json;

    let social = FakeSocial::new(view());
    let overlay = Overlay::new(OverlayConfig {
        enabled: false,
        ..OverlayConfig::default()
    });
    assert!(overlay.wants_mod());
    let seen = statuses(&overlay);
    let endpoint = overlay.start(social.clone(), "t").await.unwrap();
    // E2: the game starts without its panel.
    assert!(
        endpoint
            .jvm_args()
            .contains(&format!("{}off", crate::PANEL_ARG))
    );
    let mut game = MockMod::connect_with(&endpoint, &["lang", "emoji", "panel-off"]).await;
    assert_eq!(game.next().await.unwrap()["t"], "welcome");
    assert_eq!(game.next().await.unwrap()["t"], "emojis", "no snapshot");
    // Nothing of the friends feature: a message makes no toast; the page keeps its notifications.
    overlay.on_social(SocialUpdate::Notice(SocialNotice::ChatMessage {
        conv: "d:f-ayse:me".into(),
        message: Box::new(message("i:31", "f-ayse", false, MessageState::Sent)),
        name: Some("Ayşe".into()),
        crew: None,
    }));
    game.send(json!({"t":"emoji","on":true})).await;
    assert_eq!(
        game.next().await.unwrap()["t"],
        "emojis",
        "no toast before it"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "never counted as connected"
    );
    assert!(!overlay.connected());

    // The panel switched on during the game changes nothing for it; the picker switched off ends
    // the session, since nothing is left for the game.
    overlay.configure(OverlayConfig::default());
    overlay.configure(OverlayConfig {
        enabled: false,
        emoji: false,
        ..OverlayConfig::default()
    });
    assert_eq!(game.next_of("bye").await["reason"], "disabled");
    assert!(!overlay.wants_mod() && !overlay.active());

    // The picker off and the panel on: the panel's game, told so on its command line.
    overlay.configure(OverlayConfig {
        emoji: false,
        ..OverlayConfig::default()
    });
    let next = overlay.start(social, "t").await.unwrap();
    let args = next.jvm_args();
    assert!(args.contains(&format!("{}off", crate::EMOJI_ARG)));
    assert!(!args.iter().any(|arg| arg.starts_with(crate::PANEL_ARG)));
    overlay.stop(ByeReason::LauncherExit).await;
}

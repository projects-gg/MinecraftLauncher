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

use crate::feed::tests::{message, view};
use crate::protocol::ByeReason;
use crate::{Endpoint, Overlay, OverlayConfig, OverlayStatus, PORT_ARG, SocialActions, TOKEN_ARG};

const WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Calls {
    sent: Vec<(String, String)>,
    discarded: Vec<String>,
    read: Vec<String>,
    dismissed: Vec<String>,
    answers: Vec<(String, SocialAnswer)>,
    refuse: Option<SocialError>,
}

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
        ChatPage {
            conv: conv.to_owned(),
            messages: vec![
                message("i:1", "f-ayse", false, MessageState::Sent),
                message("i:2", "me", true, MessageState::Sent),
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

    fn chat_discard(&self, key: &str) {
        self.calls().discarded.push(key.to_owned());
    }

    fn chat_read(&self, conv: &str) {
        self.calls().read.push(conv.to_owned());
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
        serde_json::json!(["friends", "chat", "calls"])
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

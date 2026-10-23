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
            "links"
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

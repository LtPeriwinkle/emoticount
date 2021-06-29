use serenity::async_trait;
use serenity::prelude::TypeMapKey;
use serenity::model::{channel::Message, gateway::Ready};
use serenity::client::{Context, EventHandler, Client};
use serenity::client::bridge::gateway::GatewayIntents;

use regex::Regex;

use sqlx::SqlitePool;

use std::sync::Arc;
use std::fs;

struct Handler;
struct Db {
    pool: SqlitePool
}

impl Db {
    pub async fn new() -> Self {
        let pool = SqlitePool::connect("emotes.db").await.expect("couldn't open DB");
        Self {
            pool
        }
    }
}

impl TypeMapKey for Db {
    type Value = Arc<Db>;
}

#[async_trait]
trait HoldsDb {
    async fn get_db(&self) -> Arc<Db>;
}

#[async_trait]
impl HoldsDb for Context {
    async fn get_db(&self) -> Arc<Db> {
        self.data.read().await.get::<Db>().unwrap().clone()
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        lazy_static! {
            static ref RE = Regex::new("<a?:(.{2,32}):([0-9]{17,19})").unwrap();
        }
        println!("{}", msg.content);
        if RE.is_match(&msg.content) {
            let db = ctx.data.read().await.get::<Db>().unwrap().clone();
            let mut conn = db.pool.acquire().await.unwrap();
            let cap = RE.captures_iter(&msg.content).next().unwrap();
            let emote_id = &cap[2];
            let emote_name = &cap[1];
            let id = emote_id.parse::<i64>().unwrap();
            let emote = sqlx::query!("SELECT * FROM emotes WHERE id=?;", id).fetch_optional(&mut conn).await.unwrap();
            sqlx::query!("INSERT INTO users VALUES (?, ?);", *msg.author.id.as_u64() as i64 /* ffs sqlite devs */, emote_id).execute(&mut conn).await.unwrap();
            if emote.is_some() {
                let emote = emote.unwrap();
                //sqlx::query!("REPLACE INTO emotes VALUES (?, ?, ?, ?);", emote_id, emote_name, emote.uses + 1, uniq).execute(&mut conn).await.unwrap();
            }
        }
    }
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("client ready");
    }
}

#[tokio::main]
async fn main() {
    let db = Db::new().await;
    let frm = serenity::framework::StandardFramework::new();
    let mut client = Client::builder(fs::read_to_string(".bot-token").unwrap()).event_handler(Handler).intents(GatewayIntents::non_privileged()).framework(frm).await.expect("client creation error");
    let mut data = client.data.write().await;
    data.insert::<Db>(Arc::new(db));
    drop(data);
    client.start().await.unwrap();
}

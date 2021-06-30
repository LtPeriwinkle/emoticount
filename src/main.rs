use serenity::async_trait;
use serenity::prelude::TypeMapKey;
use serenity::model::{channel::{Message, Reaction, ReactionType}, gateway::Ready};
use serenity::client::{Context, EventHandler, Client};
use serenity::client::bridge::gateway::GatewayIntents;

use regex::Regex;

use sqlx::SqlitePool;

use std::sync::Arc;
use std::fs;
use std::convert::TryInto;

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
        lazy_static::lazy_static! {
            static ref RE: Regex = Regex::new("(<a?:(.{2,32}):([0-9]{17,19})>)").unwrap();
        }
        println!("{}", msg.content);
        if RE.is_match(&msg.content) {
            // have to use idiot i64 because stupid sqlite won't take a u64
            let mut in_msg: Vec<i64> = vec![];
            let db = ctx.get_db().await;
            let mut conn = db.pool.acquire().await.unwrap();
            let (mut emote_id, mut emote_name): (&str, &str);
            for cap in RE.captures_iter(&msg.content) {
                emote_id = &cap[3];
                emote_name = &cap[2];
                let id = emote_id.parse::<i64>().unwrap();
                let emote = sqlx::query!("SELECT * FROM emotes WHERE id=?;", id).fetch_optional(&mut conn).await.unwrap();
                if emote.is_some() {
                    let emote = emote.unwrap();
                    let uses = emote.uses + 1;
                    let uniq = if in_msg.contains(&id) {emote.uniq} else {emote.uniq + 1};
                    sqlx::query!("REPLACE INTO emotes (id, name, uses, uniq) VALUES (?, ?, ?, ?);", id, emote_name, uses, uniq).execute(&mut conn).await.unwrap();
                } else {
                    sqlx::query!("INSERT INTO emotes (id, name, uses, uniq) VALUES (?, ?, 1, 1)", id, emote_name).execute(&mut conn).await.unwrap();
                }
                in_msg.push(id);
            }
        }
    }
    async fn reaction_add(&self, ctx: Context, add_reaction: Reaction) {
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await.unwrap();
        if let ReactionType::Custom {id, name: Some(n), ..} = add_reaction.emoji {
            let id: i64 = id.0.try_into().unwrap();
            let emote = sqlx::query!("SELECT reacts FROM emotes WHERE id=?", id).fetch_optional(&mut conn).await.unwrap();
            if emote.is_some() {
                let emote = emote.unwrap();
                let reacts = emote.reacts + 1;
                sqlx::query!("REPLACE INTO emotes (id, reacts) VALUES (?, ?)", id, reacts).execute(&mut conn).await.unwrap();
            } else {
                let name = &n;
                sqlx::query!("INSERT INTO emotes (id, name, reacts) VALUES (?, ?, ?)", id, name, 1).execute(&mut conn).await.unwrap();
            }
        }
    }
    async fn ready(&self, _ctx: Context, _ready: Ready) {
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

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
use std::collections::HashMap;

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
            let mut in_msg: HashMap<i64, (i64, i64, String)> = HashMap::new();
            let db = ctx.get_db().await;
            let mut conn = db.pool.acquire().await.unwrap();
            let (mut emote_id, mut emote_name): (&str, &str);
            let (mut uses, mut uniq): (i64, i64);
            let mut id: i64;
            for cap in RE.captures_iter(&msg.content) {
                emote_id = &cap[3];
                emote_name = &cap[2];
                let name = format!(":{}:", emote_name);
                id = emote_id.parse::<i64>().unwrap();
                if !in_msg.contains_key(&id) {
                    let emote = sqlx::query!("SELECT * FROM emotes WHERE id=?;", id).fetch_optional(&mut conn).await.unwrap();
                    if emote.is_some() {
                        let emote = emote.unwrap();
                        uses = emote.uses + 1;
                        uniq = emote.uniq;
                    } else {
                        uses = 1;
                        uniq = 1;
                    }
                    in_msg.insert(id, (uses, uniq, name));
                } else {
                    let emote = in_msg.get(&id).unwrap().clone();
                    in_msg.insert(id, (emote.0 + 1, emote.1, emote.2));
                }
            }
            for (id, stats) in in_msg.iter() {
                sqlx::query!("INSERT OR REPLACE INTO emotes (id, name, uses, uniq) VALUES (?, ?, ?, ?)", id, stats.2, stats.0, stats.1).execute(&mut conn).await.unwrap();
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

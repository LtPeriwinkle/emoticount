use serenity::async_trait;
use serenity::prelude::TypeMapKey;
use serenity::model::{channel::Message, gateway::Ready};
use serenity::client::{Context, EventHandler, Client};
use serenity::client::bridge::gateway::GatewayIntents;

use regex::Regex;

use sqlx::SqlitePool;

use std::sync::Arc;
use std::fs::File;
use std::io::Read;

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
        println!("{}", msg.content);
    }
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("client ready");
    }
}

#[tokio::main]
async fn main() {
    let db = Db::new().await;
    let mut f = std::fs::File::open(".bot-token").unwrap();
    let mut tok: Vec<u8> = vec![];
    f.read(&mut tok).unwrap();
    let mut client = Client::builder(String::from_utf8_lossy(&tok[..])).event_handler(Handler).intents(GatewayIntents::non_privileged()).await.expect("client creation error");
    let mut data = client.data.write().await;
    data.insert::<Db>(Arc::new(db));
    drop(data);
    client.start().await.unwrap();
}

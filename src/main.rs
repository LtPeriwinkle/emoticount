/*  Copyright 2021 LtPeriwinkle
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use serenity::async_trait;
use serenity::client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler};
use serenity::framework::standard::{macros::{command, group}, CommandResult};
use serenity::model::{
    channel::{Message, Reaction, ReactionType},
    gateway::Ready,
    id::EmojiId
};
use serenity::prelude::TypeMapKey;
use serenity::utils::Colour;

use regex::Regex;

use sqlx::SqlitePool;

use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::sync::Arc;

struct Handler;
struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn new() -> Self {
        let pool = SqlitePool::connect("emotes.db")
            .await
            .expect("couldn't open DB");
        Self { pool }
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
            static ref RE: Regex = Regex::new(r"(<a?:(\w{2,32}):([0-9]{17,19})>)").unwrap();
            static ref ANIMATED: Regex = Regex::new(r"<a:\w{2,32}:[0-9]{17,19}>").unwrap();
        }
        let bot_id = ctx.http.get_current_user().await.unwrap().id;
        if msg.author.id != bot_id && RE.is_match(&msg.content) {
            // have to use idiot i64 because stupid sqlite won't take a u64
            let mut in_msg: HashMap<i64, (i64, i64, String, bool)> = HashMap::new();
            let db = ctx.get_db().await;
            let mut conn = db.pool.acquire().await.unwrap();
            let (mut emote_id, mut emote_name): (&str, &str);
            let (mut uses, mut uniq): (i64, i64);
            let mut id: i64;
            let mut animated: bool;
            for cap in RE.captures_iter(&msg.content) {
                emote_id = &cap[3];
                emote_name = &cap[2];
                let name = format!(":{}:", emote_name);
                id = emote_id.parse::<i64>().unwrap();
                if !in_msg.contains_key(&id) {
                    let emote = sqlx::query!("SELECT * FROM emotes WHERE id=?;", id)
                        .fetch_optional(&mut conn)
                        .await
                        .unwrap();
                    if emote.is_some() {
                        let emote = emote.unwrap();
                        uses = emote.uses + 1;
                        uniq = emote.uniq + 1;
                        animated = emote.animated == 1;
                    } else {
                        uses = 1;
                        uniq = 1;
                        animated = ANIMATED.is_match(&cap[0]);
                    }
                    in_msg.insert(id, (uses, uniq, name, animated));
                } else {
                    let emote = in_msg.get(&id).unwrap().clone();
                    in_msg.insert(id, (emote.0 + 1, emote.1, emote.2, emote.3));
                }
            }
            for (id, stats) in in_msg.iter() {
                sqlx::query!(
                    "INSERT OR REPLACE INTO emotes (id, name, uses, uniq, animated) VALUES (?, ?, ?, ?, ?);",
                    id,
                    stats.2,
                    stats.0,
                    stats.1,
                    stats.3
                )
                .execute(&mut conn)
                .await
                .unwrap();
            }
        }
    }
    async fn reaction_add(&self, ctx: Context, add_reaction: Reaction) {
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await.unwrap();
        if let ReactionType::Custom {
            id, name: Some(n), animated
        } = add_reaction.emoji
        {
            let id: i64 = id.0.try_into().unwrap();
            let emote = sqlx::query!("SELECT name, reacts FROM emotes WHERE id=?;", id)
                .fetch_optional(&mut conn)
                .await
                .unwrap();
            if emote.is_some() {
                let emote = emote.unwrap();
                let reacts = emote.reacts + 1;
                sqlx::query!("REPLACE INTO emotes (id, name, reacts) VALUES (?, ?, ?);", id, emote.name, reacts)
                    .execute(&mut conn)
                    .await
                    .unwrap();
            } else {
                let name = &n;
                let animated: u8 = if animated {1} else {0};
                sqlx::query!(
                    "INSERT INTO emotes (id, name, reacts, animated) VALUES (?, ?, ?, ?);",
                    id,
                    name,
                    1,
                    animated,
                )
                .execute(&mut conn)
                .await
                .unwrap();
            }
        }
    }
    async fn ready(&self, _ctx: Context, _ready: Ready) {
        println!("client ready");
    }
}

#[command]
#[only_in(guilds)]
async fn topemotes(ctx: &Context, msg: &Message) -> CommandResult {
    let emotes_guild = msg.guild(&ctx.cache).await;
    if emotes_guild.is_some() {
        let emotes_guild = emotes_guild.unwrap().emojis;
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await?;
        let emotes_db = sqlx::query!("SELECT id, name, uses, uniq, animated FROM emotes ORDER BY uses DESC;").fetch_all(&mut conn).await?;
        msg.channel_id.send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.title("Top emotes usage:");
                let mut num = 1;
                for emote in emotes_db {
                    if num < 15 && emotes_guild.contains_key(&EmojiId::from(emote.id as u64)) {
                        e.field(format!("<{}{}{}> ({})", if emote.animated == 1 {"a"} else {""}, emote.name, emote.id, emote.name), format!("Uses: {}; Unique: {}.", emote.uses, emote.uniq), true);
                        num += 1;
                    }
                }
                e.color(Colour::from_rgb(0, 43, 54))
                })
        }).await.unwrap();
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn bottomemotes(ctx: &Context, msg: &Message) -> CommandResult {
    let emotes_guild = msg.guild(&ctx.cache).await;
    if emotes_guild.is_some() {
        let emotes_guild = emotes_guild.unwrap().emojis;
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await?;
        let emotes_db = sqlx::query!("SELECT id, name, uses, uniq, animated FROM emotes ORDER BY uses ASC;").fetch_all(&mut conn).await?;
        msg.channel_id.send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.title("Bottom emotes usage:");
                let mut num = 1;
                for emote in emotes_db {
                    if num < 15 && emotes_guild.contains_key(&EmojiId::from(emote.id as u64)) {
                        e.field(format!("<{}{}{}> ({})", if emote.animated == 1 {"a"} else {""}, emote.name, emote.id, emote.name), format!("Uses: {}; Unique: {}.", emote.uses, emote.uniq), true);
                        num += 1;
                    }
                }
                e.color(Colour::from_rgb(0, 43, 54))
                })
        }).await?;
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn topreacts(ctx: &Context, msg: &Message) -> CommandResult {
    let emotes_guild = msg.guild(&ctx.cache).await;
    if emotes_guild.is_some() {
        let emotes_guild = emotes_guild.unwrap().emojis;
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await?;
        let emotes_db = sqlx::query!("SELECT id, name, reacts, animated FROM emotes WHERE reacts != 0 ORDER BY reacts DESC;").fetch_all(&mut conn).await?;
        msg.channel_id.send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.title("Top reactions usage:");
                let mut num = 1;
                for emote in emotes_db {
                    if num < 15 && emotes_guild.contains_key(&EmojiId::from(emote.id as u64)) {
                        e.field(format!("<{}:{}:{}> ({})", if emote.animated == 1 {"a"} else {""}, emote.name, emote.id, emote.name), format!("Reactions: {}", emote.reacts), true);
                        num += 1;
                    }
                }
                e.color(Colour::from_rgb(0, 43, 54))
                })
        }).await?;
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn bottomreacts(ctx: &Context, msg: &Message) -> CommandResult {
    let emotes_guild = msg.guild(&ctx.cache).await;
    if emotes_guild.is_some() {
        let emotes_guild = emotes_guild.unwrap().emojis;
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await.unwrap();
        let emotes_db = sqlx::query!("SELECT id, name, reacts, animated FROM emotes WHERE reacts != 0 ORDER BY reacts ASC;").fetch_all(&mut conn).await.unwrap();
        msg.channel_id.send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.title("Bottom reactions usage:");
                let mut num = 1;
                for emote in emotes_db {
                    if num < 15 && emotes_guild.contains_key(&EmojiId::from(emote.id as u64)) {
                        e.field(format!("<{}:{}:{}> (:{}:)", if emote.animated == 1 {"a"} else {""}, emote.name, emote.id, emote.name), format!("Reactions: {}", emote.reacts), true);
                        num += 1;
                    }
                }
                e.color(Colour::from_rgb(0, 43, 54))
                })
        }).await?;
    }
    Ok(())
}

#[command]
#[only_in(guilds)]
async fn zero(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.send_message(&ctx.http, |m| m.content("please wait")).await?;
    let emotes_guild = msg.guild(&ctx.cache).await;
    if emotes_guild.is_some() {
        let emotes_guild = emotes_guild.unwrap().emojis;
        let db = ctx.get_db().await;
        let mut conn = db.pool.acquire().await.unwrap();
        let emotes_db = sqlx::query!("SELECT id FROM emotes;").fetch_all(&mut conn).await.unwrap();
        let emotes_db: Vec<i64> = emotes_db.iter().map(|v| v.id).collect();
        msg.channel_id.send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.title("Emotes with no uses:");
                let mut num = 1;
                for (_, emote) in emotes_guild {
                    if num < 25 && !emotes_db.contains(&(emote.id.0 as i64)) {
                        e.field(format!("<{}:{}:{}>", if emote.animated {"a"} else {""}, emote.name, emote.id.0), format!("(:{}:)", emote.name) , true);
                        num += 1;
                    }
                }
                e.color(Colour::from_rgb(0, 43, 54))
            })
        }).await?;
    }
    Ok(())
}

#[group]
#[commands(topemotes, bottomemotes, topreacts, bottomreacts, zero)]
struct EmoteCommands;

#[tokio::main]
async fn main() {
    let db = Db::new().await;
    let frm = serenity::framework::StandardFramework::new().configure(|c| c.prefix(";")).group(&EMOTECOMMANDS_GROUP);
    let mut client = Client::builder(fs::read_to_string(".bot-token").unwrap())
        .event_handler(Handler)
        .intents(GatewayIntents::non_privileged())
        .framework(frm)
        .await
        .expect("client creation error");
    let mut data = client.data.write().await;
    data.insert::<Db>(Arc::new(db));
    drop(data);
    client.start().await.unwrap();
}

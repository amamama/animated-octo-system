use std::fs;
use std::{env, path::Path};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`').add(b'#');

use serde_json::Value;

use failure::Fallible;

use headless_chrome::{protocol::page::ScreenshotFormat, Browser, LaunchOptionsBuilder};

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
    http::AttachmentType,
};

macro_rules! template {
    () => {"http://wonder.wisdom-guild.net/{}/{}/"}
}

async fn post_scryfall(name: &str) ->reqwest::Result<String> {
    let resp = reqwest::get(&format!("https://api.scryfall.com/cards/named?fuzzy={}", utf8_percent_encode(name, &FRAGMENT).to_string())).await?.text().await?;
    return Ok(resp);
}

async fn get_cardname(name: &str) -> Result<String, &str> {
    let resp = post_scryfall(name).await;
    match resp {
        Ok(json_text) => {
           // println!("{:#?}", json_text);
            match serde_json::from_str::<Value>(&json_text) {
                Ok(v) => {
                    if v["object"] == "card" {
                        if let serde_json::Value::String(name) = &v["name"] {
                            if name.contains(" // ") {
                                let v: Vec<&str> = name.split(" // ").collect();
                                return Ok(v[0].to_string());
                            } else {
                                return Ok(name.to_string());
                            }
                        } else {
                            return Err("name prop ga nai");
                        }
                    } else {
                        return Err("mitsukaranakatta");
                    }
                },
                Err(e) => {
                    return Err("parse dekinakatta");
                }
            }
        },
        Err(e) => {
            return Err("post ga okasii");
        }
    }
}

fn get_ss(name: &str) -> Fallible<(String, String)> {
    let options = LaunchOptionsBuilder::default()
        .build()
        .expect("Couldn't find appropriate Chrome binary.");
    let browser = Browser::new(options)?;
    let tab = browser.wait_for_initial_tab()?;
    // Browse to the WebKit-Page and take a screenshot of the infobox.
    let png_data = tab
        .navigate_to(&format!(template!(), "graph", name))?
        .wait_for_element("#wg-wonder-graph-large.wg-wonder-graph.jqplot-target")?
        .capture_screenshot(ScreenshotFormat::PNG)?;
    fs::write("graph.png", &png_data)?;
    println!("Screenshots successfully created.");

    let price_table = tab
        .navigate_to(&format!(template!(), "price", name))?
        .wait_for_element("table.wg-statistics.wg-wonder-price-statistics")?;
    let min_price = price_table.call_js_fn("function () { return this.children[0].children[0].children[1].children[0].textContent; }", false)?.value.unwrap();
    let trim_avg = price_table.call_js_fn("function () { return this.children[0].children[0].children[3].children[0].textContent; }", false)?.value.unwrap();
    println!("kusobaka {}, {};", min_price, trim_avg);

    Ok((min_price.as_str().unwrap().to_string(), trim_avg.as_str().unwrap().to_string()))
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content.starts_with("~~") {
            println!("get message");
            let name = msg.content.trim_start_matches('~');
            println!("name = {};", name);
            let cardname = get_cardname(name).await;
            match cardname {
                Ok(cardname) => {
                    let encoded = &utf8_percent_encode(&cardname, &FRAGMENT).to_string();
                    println!("cardname = {};", cardname);
                    println!("encoded = {};", encoded);
                    match get_ss(&encoded) {
                        Ok((min, avg)) => {
                            println!("{}, {};", &min, &avg);
                            let msg = msg.channel_id.send_message(&ctx.http, |m| {
                                m.content(format!(template!(), "price", &encoded));
                                m.embed(|e| {
                                    e.title(&cardname);
                                    e.description("price and graph");
                                    e.image("attachment://graph.png");
                                    e.fields(vec![
                                             ("最安",&min, true),
                                             ("トリム平均",&avg, true),
                                    ]);
                                    e
                                });
                                m.add_file(AttachmentType::Path(Path::new("./graph.png")));
                                m
                            }).await;

                            if let Err(why) = msg {
                                println!("Error sending message: {:?}", why);
                            }
                        },
                        Err(e) => {
                            println!("get ss dekinakatta");
                            let msg = msg.channel_id.send_message(&ctx.http, |m| {
                                m.content(e);
                                m
                            }).await;

                            if let Err(why) = msg {
                                println!("Error sending message: {:?}", why);
                            }
                        },
                    }
                },
                Err(e) => {
                    println!("get cardname dekinakatta");
                    let msg = msg.channel_id.send_message(&ctx.http, |m| {
                        m.content(e);
                        m
                    }).await;

                    if let Err(why) = msg {
                        println!("Error sending message: {:?}", why);
                    }
                },
                // The create message builder allows you to easily create embeds and messages
                // using a builder syntax.
                // This example will create a message that says "Hello, World!", with an embed that has
                // a title, description, three fields, and a footer.
            }
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}


#![feature(plugin)]
#![plugin(serde_macros)]
#![feature(custom_derive, custom_attribute, stmt_expr_attributes)]
extern crate discord;
extern crate serde;
extern crate serde_json;
extern crate hyper;
extern crate url;
extern crate discord_bot;
extern crate cleverbot_io;

use std::fs::File;
use std::io::Read;
use discord::{Discord, State};
use discord::model::{ChannelId, Event, UserId};
use url::Url;
use discord_bot::shortcuts::{info, remove_quote, send_discord_message, warn, warning};
use cleverbot_io::Cleverbot;

fn main() {
    // Read and set config vars
    let mut file = File::open("config.json").unwrap();
    let mut config = String::new();
    file.read_to_string(&mut config).unwrap();

    #[derive(Deserialize)]
    pub struct Config {
        pub discord_bot_token: String,
        pub server_welcome_message: String,
        pub cleverbot_api_user: String,
        pub cleverbot_api_key: String,
    }

    let config_json = serde_json::from_str::<Config>(&config).unwrap();
    let bot_tokens = config_json.discord_bot_token;
    let welcome_messages = config_json.server_welcome_message;
    let api_user = config_json.cleverbot_api_user;
    let api_key = config_json.cleverbot_api_key;

    info("bot-token has been set to [REDACTED] from config");
    info(&format!("welcome-message has been set to {} from the config",
                  welcome_messages));
    info(&format!("api_user {}", api_user));
    info(&format!("api-key {}", api_key));

    // Login to the API
    let discord = Discord::from_bot_token(&bot_tokens).expect("Login Failed, Please make sure that you set a correct bot token.");

    // establish websocket and voice connection
    let (mut connection, ready) = discord.connect().expect("connect failed");
    println!("[Ready] {} is serving {} servers",
             ready.user.username,
             ready.servers.len());
    let mut state = State::new(ready);

    // receive events forever
    loop {
        let event = match connection.recv_event() {
            Ok(event) => event,
            Err(err) => {
                warning(&format!("Receive error: {:?}", err));
                if let discord::Error::WebSocket(..) = err {
                    // Handle the websocket connection being dropped
                    let (new_connection, ready) = discord.connect().expect("connect failed");
                    connection = new_connection;
                    state = State::new(ready);
                    println!("[Ready] Reconnected successfully.");
                }
                if let discord::Error::Closed(..) = err {
                    break;
                }
                continue;
            }
        };
        state.update(&event);

        match event {
            Event::MessageCreate(message) => {
                use std::ascii::AsciiExt;
                // safeguard: stop if the message is from us
                if message.author.id == state.user().id {
                    continue;
                }

                // reply to a command if there was one
                let mut split = message.content.split(' ');
                let first_word = split.next().unwrap_or("");
                let argument = split.next().unwrap_or("");

                if first_word.eq_ignore_ascii_case("!help") {
                    if argument.eq_ignore_ascii_case("dj") {
                        send_discord_message(&discord,
                                             &message.channel_id,
                                             "``!dj`` Plays YouTube videos in Voice Chat:\n\n\
                                             ``!dj stop`` Stops the current playing song.\n\
                                             ``!dj quit`` Stops the current playing song, and exits the Voice Chat.");
                    } else {
                        send_discord_message(&discord,
                                             &message.channel_id,
                                             &format!("Here's the help that {} wanted:\n\n\
                                                      ``!dj`` Plays YouTube videos in Voice Chat. See ``!help dj`` for more info\n\n\
                                                      ``!catfacts`` Lists a random fact about cats.\n\n\
                                                      ``!help`` Shows this output.",
                                                      message.author.id.mention()));
                    }
                } else if first_word.eq_ignore_ascii_case("!dj") {
                    let vchan = state.find_voice_user(message.author.id);
                    if argument.eq_ignore_ascii_case("stop") {
                        vchan.map(|(sid, _)| connection.voice(sid).stop());
                    } else if argument.eq_ignore_ascii_case("quit") {
                        vchan.map(|(sid, _)| connection.drop_voice(sid));
                    } else {
                        let output = if let Some((server_id, channel_id)) = vchan {
                            match discord::voice::open_ytdl_stream(argument) {
                                Ok(stream) => {
                                    let voice = connection.voice(server_id);
                                    voice.set_deaf(true);
                                    voice.connect(channel_id);
                                    voice.play(stream);
                                    String::new()
                                }
                                Err(error) => format!("Error: {}", error),
                            }
                        } else {
                            "You must be in a voice channel to DJ".to_owned()
                        };
                        if output.is_empty() {
                            warn(discord.send_message(&message.channel_id, &output, "", false));
                        }
                    }
                } else if first_word.eq_ignore_ascii_case("!catfacts") {
                    // Construct the URL you want to access
                    let url = "http://catfacts-api.appspot.com/api/facts?number=1"
                        .parse::<Url>()
                        .expect("Unable to parse URL");

                    // Initialize the Hyper client and make the request.
                    let client = hyper::Client::new();
                    let mut response = client.get(url).send().unwrap();

                    // Initialize a string buffer, and read the response into it.
                    let mut result = String::new();
                    response.read_to_string(&mut result).unwrap();

                    // Deserialize the result.
                    #[derive(Deserialize)]
                    pub struct CatFacts {
                        pub facts: Vec<String>,
                        pub success: bool,
                    }
                    let cat_fact =
                        serde_json::from_str::<CatFacts>(&result).unwrap().facts.pop().unwrap();
                    let cat_facts = remove_quote(&cat_fact);

                    send_discord_message(&discord,
                                         &message.channel_id,
                                         &format!("{}:\n {:?}",
                                                  message.author.id.mention(),
                                                  cat_facts));
                } else if first_word.eq_ignore_ascii_case("!cleverbot") {
                    let mut bot = Cleverbot::new(api_user.clone(), api_key.clone(), None).unwrap();
                    #[allow(useless_format)]
                    send_discord_message(&discord, &message.channel_id, &format!("{}", bot.say(&format!("{}", message.content)).unwrap()));
                } else if first_word.eq_ignore_ascii_case("!quit") {
                    if message.author.id == UserId(77812253511913472) {
                        send_discord_message(&discord, &message.channel_id, "Shutting Down...");
                        info(&format!("{} has told me to quit.", message.author.name));
                        break;
                    } else {
                        send_discord_message(&discord,
                                             &message.channel_id,
                                             "Your not authorized to do that");
                        warning(&format!("{} with the {:?} tried to kill me.",
                                         message.author.name,
                                         message.author.id));
                    }
                }
            }
            Event::VoiceStateUpdate(server_id, _) => {
                // If someone moves/hangs up, and we are in a voice channel,
                if let Some(cur_channel) = connection.voice(server_id).current_channel() {
                    // and our current voice channel is empty, disconnect from voice
                    if let Some(srv) = state.servers().iter().find(|srv| srv.id == server_id) {
                        if srv.voice_states
                            .iter()
                            .filter(|vs| vs.channel_id == Some(cur_channel))
                            .count() <= 1 {
                            connection.voice(server_id).disconnect();
                        }
                    }
                }
            }
            Event::ServerMemberAdd(server_joined_id, member) => {
                let channel_id = ChannelId(server_joined_id.0);

                for server in state.servers() {
                    if server.id == server_joined_id {
                        send_discord_message(&discord,
                                             &channel_id,
                                             &format!("Welcome {} to {}! {}",
                                                      member.user.name,
                                                      server.name,
                                                      welcome_messages));
                    }
                }
            }
            _ => {} // discard other events
        }
    }
}

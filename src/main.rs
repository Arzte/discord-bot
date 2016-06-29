#![feature(plugin)]
#![plugin(serde_macros)]
#![feature(custom_derive)]
#![feature(custom_attribute)]
extern crate discord;
extern crate serde;
extern crate serde_json;
extern crate hyper;
extern crate url;

use std::fs::File;
use std::io::Read;
use discord::{Discord, State};
use discord::model::{Event, ChannelId, UserId};
use url::Url;

fn main() {
    // Read and set config vars
    let mut file = File::open("config.json").unwrap();
    let mut config = String::new();
    file.read_to_string(&mut config).unwrap();

    let json: serde_json::Value = serde_json::from_str(&config).unwrap();
    let bot_tokens = json.find_path(&["bot-token"]).unwrap();
    let welcome_message = json.find_path(&["welcome-message"]).unwrap();

    info(&format!("[bot-token has been set to [REDACTED] from config"));
    info(&format!("welcome-message has been set to {} from the config",
                  welcome_message));

    // Login to the API
    let discord = Discord::from_bot_token(bot_tokens.as_string().unwrap()).expect("Login Fail");

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
                let mut split = message.content.split(" ");
                let first_word = split.next().unwrap_or("");
                let argument = split.next().unwrap_or("");

                if first_word.eq_ignore_ascii_case("!help") {
                    if argument.eq_ignore_ascii_case("dj") {
                        try_twice(&discord,
                                  &message.channel_id,
                                  &format!("``!dj`` Plays YouTube videos in Voice \
                                            Chat:\n\n``!dj stop`` Stops the current playing \
                                            song.\n``!dj quit`` Stops the current playing song, \
                                            and exits the Voice Chat."));
                    } else {
                        try_twice(&discord,
                                  &message.channel_id,
                                  &format!("Here's the help that {} wanted:\n\n``!dj`` Plays \
                                            YouTube videos in Voice Chat. See ``!help dj`` for \
                                            more info\n\n``!catfacts`` Lists a random fact \
                                            about cats.\n\n``!help`` Shows this output.",
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
                            match discord::voice::open_ytdl_stream(&argument) {
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
                        if output.len() > 0 {
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
                    let cat_facts =
                        serde_json::from_str::<CatFacts>(&result).unwrap().facts.pop().unwrap();

                    try_twice(&discord,
                              &message.channel_id,
                              &format!("{}:\n {:?}", message.author.id.mention(), cat_facts));
                } else if first_word.eq_ignore_ascii_case("!quit") {
                    if message.author.id == UserId(77812253511913472) {
                        try_twice(&discord, &message.channel_id, "Shutting Down...");
                        info(&format!("{} has told me to quit.", message.author.name));
                        std::process::exit(0);
                    } else {
                        try_twice(&discord,
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
                        try_twice(&discord,
                                  &channel_id,
                                  &format!("Welcome {} to {}! {}",
                                           member.user.name,
                                           server.name,
                                           welcome_message));
                    }
                }
            }
            _ => {} // discard other events
        }
    }
}

fn warn<T, E: ::std::fmt::Debug>(result: Result<T, E>) {
    match result {
        Ok(_) => {}
        Err(err) => println!("[Warning] {:?}", err),
    }
}

fn try_twice(discord: &Discord, channel: &ChannelId, message: &str) {
    let result = discord.send_message(channel, message, "", false);
    match result {
        Ok(_) => {} // nothing to do, it was sent - the `Ok()` contains a `Message` if you want it
        Err(discord::Error::RateLimited(milliseconds)) => {
            let sleep_duration = std::time::Duration::from_millis(milliseconds);

            warning(&format!("We were rate limited for {:?} milliseconds.",
                             sleep_duration));
            std::thread::sleep(sleep_duration);
            try_twice(discord, channel, message);
        }
        _ => {} // discard all other events
    }
}

fn remove_quote(text: &str) -> String {
    let mut start_quote = None;
    let mut end_quote = None;
    let mut bytes: Vec<u8> = text.bytes().collect();

    for (i, &c) in bytes.iter().enumerate() {
        if c == b'"' {
            start_quote = Some(i);
            break;
        }
    }

    for (i, &c) in bytes.iter().enumerate().rev() {
        if c == b'"' {
            end_quote = Some(i);
            break;
        }
    }

    bytes.remove(end_quote.unwrap());
    bytes.remove(start_quote.unwrap());

    String::from_utf8(bytes).unwrap()
}

fn remove_block_brace(text: &str) -> String {
    let mut start_brace = None;
    let mut end_brace = None;
    let mut bytes: Vec<u8> = text.bytes().collect();

    for (i, &c) in bytes.iter().enumerate() {
        if c == b'[' {
            start_brace = Some(i);
            break;
        }
    }

    for (i, &c) in bytes.iter().enumerate().rev() {
        if c == b']' {
            end_brace = Some(i);
            break;
        }
    }

    bytes.remove(end_brace.unwrap());
    bytes.remove(start_brace.unwrap());

    String::from_utf8(bytes).unwrap()
}

fn warning(output: &str) {
    println!("[Warning] {}", output);
}
fn info(output: &str) {
    println!("[Info] {}", output);
}

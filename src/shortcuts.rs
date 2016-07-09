extern crate discord;
extern crate std;

use self::discord::Discord;
use self::discord::model::ChannelId;

pub fn warn<T, E: ::std::fmt::Debug>(result: Result<T, E>) {
    match result {
        Ok(_) => {}
        Err(err) => println!("[Warning] {:?}", err),
    }
}

#[allow(unknown_lints)]
#[allow(match_same_arms)]
pub fn send_discord_message(discord: &Discord, channel: &ChannelId, message: &str) {
    let result = discord.send_message(channel, message, "", false);
    match result {
        Ok(_) => {} // nothing to do, it was sent - the `Ok()` contains a `Message` if you want it
        Err(discord::Error::RateLimited(milliseconds)) => {
            let sleep_duration = std::time::Duration::from_millis(milliseconds);

            warning(&format!("We were rate limited for {:?} milliseconds.",
                             sleep_duration));
            std::thread::sleep(sleep_duration);
            send_discord_message(discord, channel, message);
        }
        _ => {} // discard all other events
    }
}

pub fn warning(output: &str) {
    println!("[Warning] {}", output);
}
pub fn info(output: &str) {
    println!("[Info] {}", output);
}

pub fn remove_quote(text: &str) -> String {
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

pub fn remove_block_brace(text: &str) -> String {
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

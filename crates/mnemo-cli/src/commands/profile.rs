use mnemo::Mnemo;

use crate::ProfileCommand;

pub async fn run(db: &Mnemo, cmd: ProfileCommand) -> anyhow::Result<()> {
    let profile = db.profile();

    match cmd {
        ProfileCommand::Set { key, value, confidence } => {
            profile.set(key.clone(), value, confidence).await?;
            println!("set {key}");
        }
        ProfileCommand::Get { key } => match profile.get(key.clone()).await? {
            Some(entry) => println!("{} = {} (confidence {:.2})", entry.key, entry.value, entry.confidence),
            None => println!("no profile entry for {key}"),
        },
        ProfileCommand::List => {
            let entries = profile.get_all().await?;
            if entries.is_empty() {
                println!("profile is empty");
            }
            for entry in entries {
                println!("{} = {} (confidence {:.2})", entry.key, entry.value, entry.confidence);
            }
        }
        ProfileCommand::Remove { key } => {
            profile.remove(key.clone()).await?;
            println!("removed {key}");
        }
    }

    Ok(())
}

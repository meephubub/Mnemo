use mnemo::{Mnemo, ProfileProposal};

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
        ProfileCommand::Propose { key, value, confidence } => match profile.propose(key.clone(), value, confidence).await? {
            ProfileProposal::Saved => println!("set {key} (confidence {confidence:.2})"),
            ProfileProposal::NeedsConfirmation => {
                println!("confidence {confidence:.2} needs confirmation; not written. Use `memory propose` to record it as a review candidate instead.")
            }
            ProfileProposal::Rejected => println!("rejected: confidence {confidence:.2} is below 0.50"),
        },
    }

    Ok(())
}

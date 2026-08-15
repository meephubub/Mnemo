use std::str::FromStr;

use mnemo::ids::MemoryId;
use mnemo::models::{MemoryStatus, MemoryType};
use mnemo::Mnemo;

use crate::{MemoryCommand, MemoryStatusArg, MemoryTypeArg};

pub async fn run(db: &Mnemo, cmd: MemoryCommand) -> anyhow::Result<()> {
    let memories = db.memories();

    match cmd {
        MemoryCommand::Add { content, r#type } => {
            if content.is_empty() {
                anyhow::bail!("no content given; usage: mnemo memory add <content...>");
            }
            let memory = memories.add(to_memory_type(r#type), content.join(" ")).await?;
            println!("added memory {} ({})", memory.id, type_label(memory.memory_type));
        }
        MemoryCommand::List { status } => {
            let entries = memories.list(status.map(to_memory_status)).await?;
            if entries.is_empty() {
                println!("no memories found");
            }
            for m in entries {
                println!(
                    "{}  [{}/{}]  importance={:.2}  {}",
                    m.id,
                    type_label(m.memory_type),
                    status_label(m.status),
                    m.importance,
                    m.content
                );
            }
        }
        MemoryCommand::Remove { id } => {
            let id = MemoryId::from_str(&id).map_err(|e| anyhow::anyhow!("invalid memory id: {e}"))?;
            memories.delete(id).await?;
            println!("removed memory {id}");
        }
    }

    Ok(())
}

fn to_memory_type(arg: MemoryTypeArg) -> MemoryType {
    match arg {
        MemoryTypeArg::Fact => MemoryType::Fact,
        MemoryTypeArg::Preference => MemoryType::Preference,
        MemoryTypeArg::Interest => MemoryType::Interest,
        MemoryTypeArg::Goal => MemoryType::Goal,
        MemoryTypeArg::Project => MemoryType::Project,
        MemoryTypeArg::Person => MemoryType::Person,
        MemoryTypeArg::Location => MemoryType::Location,
        MemoryTypeArg::Routine => MemoryType::Routine,
        MemoryTypeArg::Decision => MemoryType::Decision,
        MemoryTypeArg::Event => MemoryType::Event,
        MemoryTypeArg::Temporary => MemoryType::Temporary,
    }
}

fn to_memory_status(arg: MemoryStatusArg) -> MemoryStatus {
    match arg {
        MemoryStatusArg::Candidate => MemoryStatus::Candidate,
        MemoryStatusArg::Active => MemoryStatus::Active,
        MemoryStatusArg::Temporary => MemoryStatus::Temporary,
        MemoryStatusArg::Superseded => MemoryStatus::Superseded,
        MemoryStatusArg::Archived => MemoryStatus::Archived,
        MemoryStatusArg::Expired => MemoryStatus::Expired,
    }
}

fn type_label(t: MemoryType) -> &'static str {
    match t {
        MemoryType::Fact => "fact",
        MemoryType::Preference => "preference",
        MemoryType::Interest => "interest",
        MemoryType::Goal => "goal",
        MemoryType::Project => "project",
        MemoryType::Person => "person",
        MemoryType::Location => "location",
        MemoryType::Routine => "routine",
        MemoryType::Decision => "decision",
        MemoryType::Event => "event",
        MemoryType::Temporary => "temporary",
    }
}

fn status_label(s: MemoryStatus) -> &'static str {
    match s {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Active => "active",
        MemoryStatus::Temporary => "temporary",
        MemoryStatus::Superseded => "superseded",
        MemoryStatus::Archived => "archived",
        MemoryStatus::Expired => "expired",
    }
}

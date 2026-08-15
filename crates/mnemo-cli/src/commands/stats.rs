use mnemo::models::MemoryStatus;
use mnemo::Mnemo;

pub async fn run(db: &Mnemo) -> anyhow::Result<()> {
    let sources = db.ingest().list_sources().await?;
    let documents = db.ingest().list_documents().await?;
    let conversations = db.conversations().list().await?;
    let profile = db.profile().get_all().await?;
    let active_memories = db.memories().list(Some(MemoryStatus::Active)).await?;
    let candidate_memories = db.memories().list(Some(MemoryStatus::Candidate)).await?;

    println!("sources:              {}", sources.len());
    println!("documents:            {}", documents.len());
    println!("conversations:        {}", conversations.len());
    println!("profile entries:      {}", profile.len());
    println!("active memories:      {}", active_memories.len());
    println!("candidate memories:   {}", candidate_memories.len());

    Ok(())
}

use std::collections::BTreeSet;

use orchy_core::{Document, Id, Message, Result, Skill, Task};

use crate::codec;
use crate::layout::topic_of;
use crate::vault::Vault;

pub(crate) async fn of_document(vault: &Vault, document: &Document) -> Result<String> {
    let slot = vault
        .layout()
        .document_slot(document.namespace(), document.title().as_str());
    vault.place(&slot, document.id()).await
}

pub(crate) async fn of_task(vault: &Vault, task: &Task) -> Result<String> {
    let slot = vault
        .layout()
        .task_slot(task.status(), task.title().as_str());
    vault.place(&slot, task.id()).await
}

pub(crate) async fn of_skill(vault: &Vault, skill: &Skill) -> Result<String> {
    let slot = vault.layout().skill_slot(skill.namespace(), skill.name());
    vault.place(&slot, skill.id()).await
}

pub(crate) async fn of_message(vault: &Vault, message: &Message) -> Result<String> {
    let folder = thread_folder(vault, message).await?;
    let slot = vault
        .layout()
        .message_slot(&folder, message.created_at(), message.from());
    vault.place(&slot, message.id()).await
}

async fn thread_folder(vault: &Vault, message: &Message) -> Result<String> {
    if message.thread() != message.id()
        && let Some(root) = vault.locate(message.thread())
    {
        return Ok(folder_of(&root.key));
    }
    let topic = topic_of(
        message.subject().map(|s| s.as_str()),
        message.body().as_str(),
    );
    let slot = vault.layout().thread_slot(&topic);
    if let Some(located) = vault.locate(message.id()) {
        let folder = folder_of(&located.key);
        if slot.fits_path(&folder) && holds_only(vault, &folder, message.thread()).await? {
            return Ok(folder);
        }
    }
    let mut n = 1;
    loop {
        let folder = slot.path(n);
        if holds_only(vault, &folder, message.thread()).await? {
            return Ok(folder);
        }
        n += 1;
    }
}

async fn holds_only(vault: &Vault, folder: &str, thread: &Id) -> Result<bool> {
    let threads = threads_in(vault, folder).await?;
    Ok(threads.iter().all(|t| t == thread))
}

async fn threads_in(vault: &Vault, folder: &str) -> Result<BTreeSet<Id>> {
    let mut threads = BTreeSet::new();
    for key in vault.blobs().list(&format!("{folder}/")).await? {
        if folder_of(&key) != folder || !vault.layout().is_markdown(&key) {
            continue;
        }
        let Ok(Some(file)) = vault.read(&key).await else {
            continue;
        };
        if let Ok(message) = codec::message_from_markdown(&file) {
            threads.insert(message.thread().clone());
        }
    }
    Ok(threads)
}

fn folder_of(key: &str) -> String {
    key.rsplit_once('/')
        .map(|(folder, _)| folder.to_owned())
        .unwrap_or_default()
}

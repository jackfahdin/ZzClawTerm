use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use zzclawterm_core::ai::AiMessage;

use crate::features::ai::panel::{AiAgentStepPresentation, AiPanelSnapshot};
use crate::features::ai::presentation::AiAgentStepKind;

/// Derived lookups; the immutable snapshot remains the sole data source.
#[derive(Default)]
pub(super) struct AiSnapshotIndex {
    messages: Arc<[Arc<AiMessage>]>,
    steps: Arc<[AiAgentStepPresentation]>,
    running: bool,
    streaming_id: Option<String>,
    message_ids: HashMap<String, usize>,
    step_positions: HashMap<u16, usize>,
    card_owners: HashMap<String, usize>,
    card_steps: HashMap<String, usize>,
    message_steps: HashMap<String, Vec<usize>>,
}

impl AiSnapshotIndex {
    pub(super) fn build(snapshot: &AiPanelSnapshot) -> Self {
        let mut index = Self {
            messages: Arc::clone(&snapshot.messages),
            steps: Arc::clone(&snapshot.agent_steps),
            running: snapshot.running,
            streaming_id: snapshot.streaming_assistant_id.clone(),
            ..Self::default()
        };
        let mut owned_cards = HashSet::new();
        for (position, message) in snapshot.messages.iter().enumerate() {
            index
                .message_ids
                .entry(message.id.clone())
                .or_insert(position);
            for card in &message.command_cards {
                index.card_owners.entry(card.id.clone()).or_insert(position);
                owned_cards.insert((message.id.as_str(), card.id.as_str()));
            }
        }
        for (position, presentation) in snapshot.agent_steps.iter().enumerate() {
            let step = &presentation.step;
            index
                .step_positions
                .entry(step.step_index)
                .or_insert(position);
            if let (Some(source), Some(card)) = (
                step.source_message_id.as_deref(),
                step.command_card_id.as_deref(),
            ) && owned_cards.contains(&(source, card))
            {
                index.card_steps.entry(card.to_string()).or_insert(position);
            }
        }
        for (position, presentation) in snapshot.agent_steps.iter().enumerate() {
            let step = &presentation.step;
            if step.kind == AiAgentStepKind::FinalAnswer
                || step
                    .command_card_id
                    .as_ref()
                    .is_some_and(|id| index.card_steps.contains_key(id))
            {
                continue;
            }
            let source = step
                .source_message_id
                .as_deref()
                .filter(|id| index.message_ids.contains_key(*id))
                .or_else(|| {
                    snapshot
                        .step_is_active(step.step_index)
                        .then_some(snapshot.streaming_assistant_id.as_deref())
                        .flatten()
                });
            if let Some(source) = source {
                index
                    .message_steps
                    .entry(source.to_string())
                    .or_default()
                    .push(position);
            }
        }
        index
    }

    pub(super) fn matches(&self, snapshot: &AiPanelSnapshot) -> bool {
        Arc::ptr_eq(&self.messages, &snapshot.messages)
            && Arc::ptr_eq(&self.steps, &snapshot.agent_steps)
            && self.running == snapshot.running
            && self.streaming_id == snapshot.streaming_assistant_id
    }

    pub(super) fn message<'a>(
        &self,
        snapshot: &'a AiPanelSnapshot,
        id: &str,
    ) -> Option<&'a AiMessage> {
        if self.matches(snapshot) {
            self.message_ids
                .get(id)
                .map(|index| snapshot.messages[*index].as_ref())
        } else {
            snapshot
                .messages
                .iter()
                .find(|message| message.id == id)
                .map(Arc::as_ref)
        }
    }

    pub(super) fn card_step(&self, id: &str) -> Option<usize> {
        self.card_steps.get(id).copied()
    }
    pub(super) fn step_position(&self, id: u16) -> Option<usize> {
        self.step_positions.get(&id).copied()
    }
    pub(super) fn card_owner(&self, id: &str) -> Option<usize> {
        self.card_owners.get(id).copied()
    }
    pub(super) fn steps(&self, id: &str) -> &[usize] {
        self.message_steps
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

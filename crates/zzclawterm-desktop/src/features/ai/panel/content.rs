use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{Context, IntoElement, div, prelude::*};
use zzclawterm_core::ai::AiMessage;

use crate::features::ai::panel::transcript::AiTranscriptRow;
use crate::features::ai::panel::{AiPanel, AiPanelSnapshot};
use crate::features::formatting::extract_think_content;
use crate::features::view_widgets::markdown::{PreparedMarkdown, prepared_markdown_view};
use crate::theme::ThemePalette;

pub(super) struct PreparedMessage {
    pub display: String,
    pub reasoning: Option<String>,
    pub answer: PreparedMarkdown,
    pub thought: PreparedMarkdown,
}

impl PreparedMessage {
    fn parse(message: &AiMessage) -> Self {
        let (display, embedded) = extract_think_content(&message.content);
        let reasoning = message
            .reasoning_content
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string)
            .or(embedded);
        Self {
            answer: PreparedMarkdown::parse(&display),
            thought: PreparedMarkdown::parse(reasoning.as_deref().unwrap_or_default()),
            display,
            reasoning,
        }
    }
}

struct CachedMessage {
    source: Arc<AiMessage>,
    prepared: Arc<PreparedMessage>,
}

#[derive(Default)]
pub(super) struct ContentCache {
    session: String,
    generation: u64,
    in_flight: bool,
    messages: HashMap<String, CachedMessage>,
    markdown: HashMap<String, Arc<PreparedMarkdown>>,
    #[cfg(test)]
    pub parses: usize,
}

fn same_source(a: &Arc<AiMessage>, b: &Arc<AiMessage>) -> bool {
    Arc::ptr_eq(a, b) || (a.content == b.content && a.reasoning_content == b.reasoning_content)
}

fn extra_sources(snapshot: &AiPanelSnapshot) -> HashSet<String> {
    snapshot
        .agent_steps
        .iter()
        .flat_map(|step| {
            [
                Some(step.step.detail.as_str()),
                step.step.thought.as_deref(),
            ]
        })
        .flatten()
        .chain(
            snapshot
                .messages
                .iter()
                .flat_map(|message| &message.command_cards)
                .chain(snapshot.command_cards.iter())
                .map(|card| card.explanation.as_str()),
        )
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect()
}

impl AiPanel {
    pub(super) fn refresh_content(&mut self, snapshot: &AiPanelSnapshot, cx: &mut Context<Self>) {
        let cache = &mut self.content;
        if cache.session != snapshot.current_ai_session_id {
            cache.session = snapshot.current_ai_session_id.clone();
            cache.generation = cache.generation.wrapping_add(1);
            cache.in_flight = false;
            cache.messages.clear();
            cache.markdown.clear();
        }
        let ids: HashSet<_> = snapshot
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        cache.messages.retain(|id, _| ids.contains(id.as_str()));
        let extras = extra_sources(snapshot);
        cache.markdown.retain(|source, _| extras.contains(source));
        if cache.in_flight {
            return;
        }
        let missing: Vec<_> = snapshot
            .messages
            .iter()
            .filter(|message| {
                !cache
                    .messages
                    .get(&message.id)
                    .is_some_and(|ready| same_source(&ready.source, message))
            })
            .cloned()
            .collect();
        let texts: Vec<_> = extras
            .into_iter()
            .filter(|text| !cache.markdown.contains_key(text))
            .collect();
        if missing.is_empty() && texts.is_empty() {
            return;
        }
        let generation = cache.generation;
        cache.in_flight = true;
        let work = cx.background_executor().spawn(async move {
            let messages: Vec<_> = missing
                .into_iter()
                .map(|source| {
                    let prepared = Arc::new(PreparedMessage::parse(&source));
                    (source, prepared)
                })
                .collect();
            let markdown: Vec<_> = texts
                .into_iter()
                .map(|source| {
                    let prepared = Arc::new(PreparedMarkdown::parse(&source));
                    (source, prepared)
                })
                .collect();
            (messages, markdown)
        });
        cx.spawn(async move |panel, cx| {
            let (messages, markdown) = work.await;
            let _ = panel.update(cx, |panel, cx| {
                if panel.content.generation != generation {
                    return;
                }
                panel.content.in_flight = false;
                let Some(snapshot) = panel.snapshot.clone() else {
                    return;
                };
                let desired: HashMap<_, _> = snapshot
                    .messages
                    .iter()
                    .map(|message| (message.id.as_str(), message))
                    .collect();
                let extras = extra_sources(&snapshot);
                let mut changed = HashSet::new();
                let mut changed_text = HashSet::new();
                #[cfg(test)]
                {
                    panel.content.parses += messages.len() + markdown.len();
                }
                for (source, prepared) in messages {
                    if desired
                        .get(source.id.as_str())
                        .is_some_and(|latest| same_source(&source, latest))
                    {
                        changed.insert(source.id.clone());
                        panel
                            .content
                            .messages
                            .insert(source.id.clone(), CachedMessage { source, prepared });
                    }
                }
                for (source, prepared) in markdown {
                    if extras.contains(&source) {
                        changed_text.insert(source.clone());
                        panel.content.markdown.insert(source, prepared);
                    }
                }
                // Only content-owning rows changed; keep every other measured height and anchor.
                let ranges: Vec<_> = panel
                    .transcript_rows
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| {
                        let dirty = match row {
                            AiTranscriptRow::Message { id, .. }
                            | AiTranscriptRow::ActivityMessage { id, .. }
                            | AiTranscriptRow::FinalMessage { id, .. } => {
                                changed.contains(id)
                                    || snapshot.index.message(&snapshot, id).is_some_and(
                                        |message| {
                                            message.command_cards.iter().any(|card| {
                                                changed_text.contains(&card.explanation)
                                            }) || snapshot.message_steps(id).any(|step| {
                                                changed_text.contains(&step.step.detail)
                                                    || step.step.thought.as_ref().is_some_and(
                                                        |text| changed_text.contains(text),
                                                    )
                                            })
                                        },
                                    )
                            }
                            AiTranscriptRow::AgentStep { index, .. } => {
                                let step = &snapshot.agent_steps[*index].step;
                                changed_text.contains(&step.detail)
                                    || step
                                        .thought
                                        .as_ref()
                                        .is_some_and(|text| changed_text.contains(text))
                            }
                            AiTranscriptRow::Command { index, .. } => {
                                changed_text.contains(&snapshot.command_cards[*index].explanation)
                            }
                            AiTranscriptRow::PendingCommand {
                                message_index,
                                card_index,
                                ..
                            } => changed_text.contains(
                                &snapshot.messages[*message_index].command_cards[*card_index]
                                    .explanation,
                            ),
                            _ => false,
                        };
                        dirty.then_some(index..index + 1)
                    })
                    .collect();
                panel.transcript_scroll.update(cx, |scroll, cx| {
                    for range in ranges {
                        scroll.remeasure_items(range, cx);
                    }
                });
                panel.refresh_content(&snapshot, cx);
                if !changed.is_empty() || !changed_text.is_empty() {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn prepared_message(&self, id: &str) -> Option<&PreparedMessage> {
        self.content
            .messages
            .get(id)
            .map(|entry| entry.prepared.as_ref())
    }

    pub(super) fn cached_markdown(
        &self,
        palette: ThemePalette,
        source: &str,
        answer: bool,
    ) -> gpui::AnyElement {
        self.content
            .markdown
            .get(source)
            .map(|prepared| prepared_markdown_view(palette, prepared, answer).into_any_element())
            .unwrap_or_else(|| div().min_w_0().child(source.to_string()).into_any_element())
    }
}

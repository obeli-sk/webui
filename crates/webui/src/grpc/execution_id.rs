use super::grpc_client::{self, ExecutionId};
use crate::{components::execution_header::ExecutionLink, util::color::generate_color_from_hash};
use std::{fmt::Display, str::FromStr};
use yew::{Html, ToHtml, html};

pub const EXECUTION_ID_INFIX: &str = ".";

impl ExecutionId {
    pub fn generate() -> grpc_client::ExecutionId {
        let ulid = ulid::Ulid::new();
        grpc_client::ExecutionId {
            id: format!("E_{ulid}"),
        }
    }

    // For the top-level ExecutionId return [(execution_id.to_string(), execution_id)].
    // For a derived ExecutionId, return [(grandparent_id.to_string(), grandparent_id), (parent_index, parent_id), .. (child_index, child_id)].
    pub fn as_hierarchy(&self) -> Vec<(String, ExecutionId)> {
        let mut execution_id = String::new();
        let mut vec = Vec::new();
        for part in self.id.split(EXECUTION_ID_INFIX) {
            execution_id = if execution_id.is_empty() {
                part.to_string()
            } else {
                format!("{execution_id}{EXECUTION_ID_INFIX}{part}")
            };
            vec.push((
                part.to_string(),
                ExecutionId {
                    id: execution_id.clone(),
                },
            ));
        }
        vec
    }

    pub fn parent_id(&self) -> Option<ExecutionId> {
        if let Some((left, _)) = self.id.rsplit_once(EXECUTION_ID_INFIX) {
            Some(ExecutionId {
                id: left.to_string(),
            })
        } else {
            None
        }
    }

    pub fn render_execution_parts(&self, hide_parents: bool, link: ExecutionLink) -> Html {
        let mut execution_id_vec = self.as_hierarchy();
        if hide_parents {
            execution_id_vec.drain(..execution_id_vec.len() - 1);
        }
        execution_id_vec
            .into_iter()
            .enumerate()
            .map(|(idx, (part, execution_id))| {
                html! {<>
                    if idx > 0 {
                        {EXECUTION_ID_INFIX}
                    }
                    {link.link(execution_id, &part)}

                </>}
            })
            .collect::<Vec<_>>()
            .to_html()
    }

    pub fn color(&self) -> String {
        use std::hash::Hasher as _;
        use std::hash::{DefaultHasher, Hash};

        let mut hasher = DefaultHasher::new();
        self.id.hash(&mut hasher);
        let hash = hasher.finish();
        generate_color_from_hash(hash)
    }
}

impl Display for grpc_client::ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl ToHtml for grpc_client::ExecutionId {
    fn to_html(&self) -> yew::Html {
        html! { &self.id }
    }
}

impl FromStr for grpc_client::ExecutionId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(grpc_client::ExecutionId { id: s.to_string() })
    }
}

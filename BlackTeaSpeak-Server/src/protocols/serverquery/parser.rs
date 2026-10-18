use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub command: String,
    pub positional_args: Vec<String>,
    pub named_args: BTreeMap<String, String>,
    pub option_groups: Vec<BTreeMap<String, String>>,
    pub flags: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResponse {
    pub rows: Vec<BTreeMap<String, String>>,
    pub error_id: u32,
    pub message: String,
    pub extra_fields: BTreeMap<String, String>,
}

impl QueryResponse {
    pub fn ok() -> Self {
        Self {
            rows: Vec::new(),
            error_id: 0,
            message: String::from("ok"),
            extra_fields: BTreeMap::new(),
        }
    }

    pub fn ok_row(row: BTreeMap<String, String>) -> Self {
        Self {
            rows: vec![row],
            error_id: 0,
            message: String::from("ok"),
            extra_fields: BTreeMap::new(),
        }
    }

    pub fn ok_rows(rows: Vec<BTreeMap<String, String>>) -> Self {
        Self {
            rows,
            error_id: 0,
            message: String::from("ok"),
            extra_fields: BTreeMap::new(),
        }
    }

    pub fn error(error_id: u32, message: impl Into<String>) -> Self {
        Self {
            rows: Vec::new(),
            error_id,
            message: message.into(),
            extra_fields: BTreeMap::new(),
        }
    }

    pub fn error_with_fields<const N: usize>(
        error_id: u32,
        message: impl Into<String>,
        extra_fields: [(&str, String); N],
    ) -> Self {
        Self {
            rows: Vec::new(),
            error_id,
            message: message.into(),
            extra_fields: extra_fields
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        }
    }
}

pub fn parse_request_line(line: &str) -> Result<CommandRequest> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("empty query line")
    }

    let mut positional_args = Vec::new();
    let mut named_args = BTreeMap::new();
    let mut option_groups = Vec::<BTreeMap<String, String>>::new();
    let mut flags = BTreeSet::new();
    let mut current_group_index = 0_usize;

    for token in tokens.iter().skip(1) {
        if let Some(flag) = token.strip_prefix('-') {
            if !flag.is_empty() {
                flags.insert(flag.to_string());
            }
            continue;
        }

        let segments = split_pipe_segments(token);
        if segments.len() == 1 {
            if let Some((key, value)) = token.split_once('=') {
                let decoded_value = decode_query_value(value);
                named_args.insert(key.to_string(), decoded_value.clone());
                ensure_option_group(&mut option_groups, current_group_index);
                option_groups[current_group_index].insert(key.to_string(), decoded_value);
                continue;
            }

            positional_args.push(decode_query_value(token));
            continue;
        }

        for (segment_index, segment) in segments.iter().enumerate() {
            if let Some((key, value)) = segment.split_once('=') {
                let decoded_value = decode_query_value(value);
                named_args.insert(key.to_string(), decoded_value.clone());
                ensure_option_group(&mut option_groups, current_group_index);
                option_groups[current_group_index].insert(key.to_string(), decoded_value);
            } else if !segment.is_empty() {
                positional_args.push(decode_query_value(segment));
            }

            if segment_index + 1 < segments.len() {
                current_group_index += 1;
            }
        }
    }

    if option_groups.len() == 1 && option_groups[0].is_empty() {
        option_groups.clear();
    }

    Ok(CommandRequest {
        command: tokens[0].to_ascii_lowercase(),
        positional_args,
        named_args,
        option_groups,
        flags,
    })
}

fn ensure_option_group(option_groups: &mut Vec<BTreeMap<String, String>>, index: usize) {
    while option_groups.len() <= index {
        option_groups.push(BTreeMap::new());
    }
}

fn split_pipe_segments(token: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut segment_start = 0;
    let mut escaped = false;

    for (index, character) in token.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if character == '\\' {
            escaped = true;
            continue;
        }

        if character == '|' {
            segments.push(&token[segment_start..index]);
            segment_start = index + character.len_utf8();
        }
    }

    segments.push(&token[segment_start..]);
    segments
}

pub fn decode_query_value(value: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;

    for character in value.chars() {
        if escaped {
            let decoded = match character {
                's' => ' ',
                'p' => '|',
                '/' => '/',
                'n' => '\n',
                't' => '\t',
                '\\' => '\\',
                other => other,
            };
            result.push(decoded);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }

    if escaped {
        result.push('\\');
    }

    result
}

pub fn encode_query_value(value: &str) -> String {
    let mut result = String::new();

    for character in value.chars() {
        match character {
            '\\' => result.push_str("\\\\"),
            ' ' => result.push_str("\\s"),
            '|' => result.push_str("\\p"),
            '/' => result.push_str("\\/"),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            other => result.push(other),
        }
    }

    result
}

pub fn render_response(response: &QueryResponse) -> String {
    let mut error_parts = vec![
        format!("error id={}", response.error_id),
        format!("msg={}", encode_query_value(&response.message)),
    ];
    error_parts.extend(
        response
            .extra_fields
            .iter()
            .map(|(key, value)| format!("{}={}", key, encode_query_value(value))),
    );
    let error_line = error_parts.join(" ");

    if response.rows.is_empty() {
        return error_line;
    }

    let rows = response
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(key, value)| format!("{}={}", key, encode_query_value(value)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("|");

    format!("{}\n{}", rows, error_line)
}

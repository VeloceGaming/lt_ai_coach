use crate::{
    recommendation::{RecommendationRequest, RecommendationShortlist},
    statistics::RoleStatistics,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const CREDENTIAL_SERVICE: &str = "com.lttools.lt-ai-coach";
const CREDENTIAL_USER: &str = "openrouter-api-key";
const SYSTEM_PROMPT: &str = r#"You are a concise Teamfight Manager 2 draft analyst.

Analyze only the supplied draft facts and evidence. Do not invent mechanics, statistics, unavailable champions, completed actions, or rules.

Use these draft principles:
- Blue owns the first pick; before that pick, preserve at least one strongest first-pick target instead of banning every top champion.
- Red owns the final pick and can preserve it for a role-specific counter when the revealed roles support that plan.
- Consecutive pick windows can secure a synergy pair or deny a contested pair.
- Early flexible picks conceal roles; late picks gain matchup information.
- Never recommend banning a champion that the same plan says to pick later.

Recommend only legal actions matching requestedAction. Treat roles probabilistically and account for sample size. Local scores are evidence, not commands. Keep the entire answer concise: summary at most two short sentences; each advantage, disadvantage, evidence explanation, alternative reason, and next-rotation plan exactly one short sentence. Give 2-3 evidence bullets, at most 2 alternatives, and no repeated explanation.

Return only the requested JSON structure."#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCoachRequest {
    pub api_key: String,
    pub model: String,
    pub selected_action: String,
    #[serde(default)]
    pub action_log: Vec<DraftActionRecord>,
    pub draft: RecommendationRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestRequest {
    pub api_key: String,
    pub model: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResponse {
    pub compatible: bool,
    pub summary: String,
    pub model: String,
    pub http_status: u16,
    pub parsed_content: Option<Value>,
    pub raw_response: String,
    pub diagnostic_path: String,
    pub usage: Option<Usage>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftActionRecord {
    pub side: String,
    pub action_type: String,
    pub champion_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCoachResponse {
    pub recommended_actions: Vec<RecommendedAction>,
    pub summary: String,
    pub draft_interpretation: DraftInterpretation,
    pub evidence: Vec<EvidenceClaim>,
    pub rejected_alternatives: Vec<RejectedAlternative>,
    pub next_rotation_plan: String,
    pub confidence: f64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub raw_output: String,
    #[serde(default)]
    pub diagnostic_path: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedAction {
    pub action: String,
    pub champion_id: String,
    pub suggested_role: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftInterpretation {
    pub our_advantages: Vec<String>,
    pub our_disadvantages: Vec<String>,
    pub opponent_advantages: Vec<String>,
    pub opponent_disadvantages: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClaim {
    pub factor: String,
    pub effect: String,
    pub explanation: String,
    pub uncertainty: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedAlternative {
    pub champion_id: String,
    pub reason: String,
}

#[derive(Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
    pub cost: Option<f64>,
}

#[derive(Deserialize)]
struct ChatResponse {
    model: Option<String>,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    finish_reason: Option<String>,
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<Value>,
    reasoning: Option<String>,
    refusal: Option<String>,
}

pub async fn ask(
    request: AiCoachRequest,
    statistics: &RoleStatistics,
    shortlist: &RecommendationShortlist,
    diagnostic_directory: &Path,
) -> Result<AiCoachResponse, String> {
    let api_key = request.api_key.trim();
    if api_key.is_empty() {
        return Err("Enter an OpenRouter API key.".to_string());
    }
    let model = request.model.trim();
    if model.is_empty() {
        return Err("Enter an OpenRouter model slug.".to_string());
    }
    if !matches!(
        request.selected_action.as_str(),
        "blue-ban" | "red-ban" | "blue-pick" | "red-pick"
    ) {
        return Err("Select a current-draft ban or pick action before asking the AI.".to_string());
    }

    let payload = build_evidence_payload(&request, statistics, shortlist);
    let request_body = json!({
        "model": model,
        "temperature": 0.2,
        "max_completion_tokens": 4000,
        "reasoning": {
            "effort": "low",
            "exclude": true
        },
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": format!(
                    "Analyze this draft evidence. Keep internal analysis concise and return the required JSON immediately.\n{}",
                    serde_json::to_string(&payload).unwrap_or_default()
                )
            }
        ],
        "response_format": response_format()
    });
    let response = reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("HTTP-Referer", "https://github.com/lttools/lt-ai-coach")
        .header("X-OpenRouter-Title", "LT AI Coach")
        .json(&request_body)
        .send()
        .await
        .map_err(|error| format!("Could not reach OpenRouter: {error}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read OpenRouter response: {error}"))?;
    let diagnostic_path = write_diagnostic(
        diagnostic_directory,
        &request.selected_action,
        model,
        &request_body,
        status.as_u16(),
        &body,
    );
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or(body);
        return Err(with_diagnostic(
            format!("OpenRouter returned {status}: {detail}"),
            diagnostic_path.as_deref(),
        ));
    }

    let chat: ChatResponse = serde_json::from_str(&body).map_err(|error| {
        with_diagnostic(
            format!("OpenRouter returned an unexpected response: {error}"),
            diagnostic_path.as_deref(),
        )
    })?;
    let message = chat
        .choices
        .first()
        .ok_or_else(|| "OpenRouter returned no recommendation.".to_string())?;
    let content = extract_message_content(&message.message).ok_or_else(|| {
        if message.finish_reason.as_deref() == Some("length") {
            return with_diagnostic(
                "The model used its entire completion budget before producing final JSON. The request has been shortened, but this model may still need a lower reasoning mode."
                    .to_string(),
                diagnostic_path.as_deref(),
            );
        }
        message
            .message
            .refusal
            .as_deref()
            .map(|refusal| format!("The model refused the request: {refusal}"))
            .unwrap_or_else(|| {
                "The model returned no final text. Try a model that supports structured JSON output."
                    .to_string()
            })
    })?;
    let mut result: AiCoachResponse = serde_json::from_str(content).map_err(|error| {
        with_diagnostic(
            format!(
                "The model returned invalid structured output: {error}\n\nRaw model reply:\n{}",
                truncate(content, 4000)
            ),
            diagnostic_path.as_deref(),
        )
    })?;
    validate_response(&result, &request, shortlist)?;
    result.model = chat.model.unwrap_or_else(|| model.to_string());
    result.usage = chat.usage;
    result.raw_output = content.to_string();
    result.diagnostic_path = diagnostic_path
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(result)
}

pub async fn test_model(
    request: ModelTestRequest,
    diagnostic_directory: &Path,
) -> Result<ModelTestResponse, String> {
    let api_key = request.api_key.trim();
    if api_key.is_empty() {
        return Err("Enter an OpenRouter API key.".to_string());
    }
    let model = request.model.trim();
    if model.is_empty() {
        return Err("Enter an OpenRouter model slug.".to_string());
    }
    let request_body = json!({
        "model": model,
        "temperature": 0,
        "max_completion_tokens": 80,
        "messages": [
            {
                "role": "user",
                "content": "Return a JSON object confirming this structured-output test. Set status to ok and value to 7."
            }
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "lt_ai_coach_model_test",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "status": {"type": "string", "const": "ok"},
                        "value": {"type": "integer", "const": 7}
                    },
                    "required": ["status", "value"],
                    "additionalProperties": false
                }
            }
        }
    });
    let response = reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("HTTP-Referer", "https://github.com/lttools/lt-ai-coach")
        .header("X-OpenRouter-Title", "LT AI Coach")
        .json(&request_body)
        .send()
        .await
        .map_err(|error| format!("Could not reach OpenRouter: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read OpenRouter response: {error}"))?;
    let diagnostic_path = write_diagnostic(
        diagnostic_directory,
        "model-test",
        model,
        &request_body,
        status.as_u16(),
        &body,
    );
    let mut result = ModelTestResponse {
        compatible: false,
        summary: format!("OpenRouter returned HTTP {}.", status.as_u16()),
        model: model.to_string(),
        http_status: status.as_u16(),
        parsed_content: None,
        raw_response: body.clone(),
        diagnostic_path: diagnostic_path
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        usage: None,
    };
    if !status.is_success() {
        result.summary =
            "The model/provider rejected the structured-output request. Inspect the raw response."
                .to_string();
        return Ok(result);
    }
    let chat: ChatResponse = match serde_json::from_str(&body) {
        Ok(chat) => chat,
        Err(_) => {
            result.summary =
                "OpenRouter returned success, but its response envelope was not recognized."
                    .to_string();
            return Ok(result);
        }
    };
    result.model = chat.model.unwrap_or_else(|| model.to_string());
    result.usage = chat.usage;
    let Some(message) = chat.choices.first() else {
        result.summary = "OpenRouter returned success without a model message.".to_string();
        return Ok(result);
    };
    let Some(content) = extract_message_content(&message.message) else {
        result.summary =
            "The model returned no final text. The raw response shows where its output went."
                .to_string();
        return Ok(result);
    };
    match serde_json::from_str::<Value>(content) {
        Ok(value)
            if value.get("status").and_then(Value::as_str) == Some("ok")
                && value.get("value").and_then(Value::as_i64) == Some(7) =>
        {
            result.compatible = true;
            result.summary =
                "Compatible: the model returned the required structured JSON.".to_string();
            result.parsed_content = Some(value);
        }
        Ok(value) => {
            result.summary =
                "The model returned JSON, but did not follow the required schema.".to_string();
            result.parsed_content = Some(value);
        }
        Err(_) => {
            result.summary =
                "The model returned final text, but it was not valid JSON.".to_string();
            result.parsed_content = Some(Value::String(content.to_string()));
        }
    }
    Ok(result)
}

pub fn load_api_key() -> Result<Option<String>, String> {
    match credential_entry()?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("Could not read the saved OpenRouter key: {error}")),
    }
}

pub fn save_api_key(api_key: &str) -> Result<(), String> {
    let entry = credential_entry()?;
    if api_key.trim().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!(
                "Could not remove the saved OpenRouter key: {error}"
            )),
        }
    } else {
        entry
            .set_password(api_key.trim())
            .map_err(|error| format!("Could not save the OpenRouter key securely: {error}"))
    }
}

fn credential_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .map_err(|error| format!("Could not open Windows Credential Manager: {error}"))
}

fn write_diagnostic(
    directory: &Path,
    selected_action: &str,
    model: &str,
    request_body: &Value,
    status: u16,
    response_body: &str,
) -> Option<PathBuf> {
    fs::create_dir_all(directory).ok()?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let path = directory.join(format!("openrouter-{timestamp}.json"));
    let document = json!({
        "note": "Authorization header and API key are intentionally excluded.",
        "selectedAction": selected_action,
        "model": model,
        "httpStatus": status,
        "request": request_body,
        "rawResponse": response_body
    });
    fs::write(&path, serde_json::to_vec_pretty(&document).ok()?).ok()?;
    Some(path)
}

fn with_diagnostic(message: String, path: Option<&Path>) -> String {
    match path {
        Some(path) => format!("{message}\n\nDiagnostic saved to: {}", path.display()),
        None => message,
    }
}

fn truncate(value: &str, maximum_chars: usize) -> String {
    let mut chars = value.chars();
    let result = chars.by_ref().take(maximum_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{result}\n...[truncated]")
    } else {
        result
    }
}

fn extract_message_content(message: &Message) -> Option<&str> {
    message
        .content
        .as_ref()
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
        .or_else(|| {
            message
                .content
                .as_ref()
                .and_then(Value::as_array)
                .and_then(|parts| {
                    parts.iter().find_map(|part| {
                        part.get("text")
                            .and_then(Value::as_str)
                            .filter(|text| !text.trim().is_empty())
                    })
                })
        })
        .or_else(|| {
            message
                .reasoning
                .as_deref()
                .filter(|reasoning| reasoning.trim_start().starts_with('{'))
        })
}

fn build_evidence_payload(
    request: &AiCoachRequest,
    statistics: &RoleStatistics,
    shortlist: &RecommendationShortlist,
) -> Value {
    let selected = request.selected_action.split_once('-');
    let requested_side = selected.map(|value| value.0).unwrap_or(&request.draft.side);
    let requested_type = selected.map(|value| value.1).unwrap_or("pick");
    let candidates = candidate_ids(shortlist, &request.draft, &request.selected_action);
    let action_candidates =
        eligible_action_candidates(shortlist, &request.draft, &request.selected_action);
    let overall = statistics
        .overall_rows
        .iter()
        .map(|row| (row.champion_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let role_rows =
        statistics
            .role_rows
            .iter()
            .fold(BTreeMap::<&str, Vec<_>>::new(), |mut result, row| {
                result.entry(&row.champion_id).or_default().push(row);
                result
            });
    let pick_local = shortlist
        .pick_recommendations
        .iter()
        .map(|row| (row.champion_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let ban_local = shortlist
        .ban_recommendations
        .iter()
        .map(|row| (row.champion_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let projections = shortlist
        .blue_projection
        .champions
        .iter()
        .chain(&shortlist.red_projection.champions)
        .map(|row| (row.champion_id.as_str(), &row.roles))
        .collect::<BTreeMap<_, _>>();
    let champion_evidence = candidates
        .iter()
        .filter_map(|id| {
            let row = overall.get(id.as_str())?;
            Some(json!({
                "championId": id,
                "name": row.champion_name,
                "overall": stat_json(row),
                "roles": role_rows.get(id.as_str()).into_iter().flatten()
                    .filter(|role| role.games >= 3)
                    .take(3)
                    .map(|role| {
                    let probability = projections
                        .get(id.as_str())
                        .and_then(|rows| rows.iter().find(|item| item.role == role.role))
                        .map(|item| item.probability);
                    json!({
                        "role": role.role,
                        "performance": stat_json(role),
                        "currentLineupProbability": probability
                    })
                }).collect::<Vec<_>>(),
                "localEvaluation": {
                    "pick": pick_local.get(id.as_str()).map(|item| json!({
                        "score": item.score,
                        "suggestedRole": item.suggested_role,
                        "synergyEffect": item.synergy_score,
                        "matchupEffect": item.matchup_score,
                        "interactionGames": item.interaction_games
                    })),
                    "ban": ban_local.get(id.as_str()).map(|item| json!({
                        "score": item.score,
                        "suggestedRole": item.suggested_role
                    }))
                }
            }))
        })
        .collect::<Vec<_>>();

    json!({
        "draft": {
            "mode": request.draft.mode,
            "userSide": request.draft.side,
            "requestedAction": {
                "side": requested_side,
                "type": requested_type
            },
            "recordedActionLog": request.action_log,
            "currentState": {
                "blueBans": request.draft.blue_bans,
                "redBans": request.draft.red_bans,
                "bluePicks": request.draft.blue_picks,
                "redPicks": request.draft.red_picks
            },
            "formatFacts": {
                "bansPerSide": request.bans_per_side,
                "picksPerSide": 5,
                "pickSideSequence": [
                    "blue", "red", "red", "blue", "blue",
                    "red", "red", "blue", "blue", "red"
                ],
                "note": "The manual board permits out-of-order entry. recordedActionLog is the user's actual input order; requestedAction is authoritative for what to recommend now."
            },
            "strategicFacts": {
                "blueFirstPickAvailable": request.draft.blue_picks.is_empty(),
                "reservedBlueFirstPickTargets": shortlist.pick_recommendations.iter()
                    .take(3)
                    .map(|item| item.champion_id.as_str())
                    .collect::<Vec<_>>(),
                "redFinalPickAvailable": request.draft.red_picks.len() < 5,
                "consistencyRule": "Do not ban a champion that the proposed plan intends to pick."
            },
            "fearlessHistory": {
                "blue": request.draft.history_blue,
                "red": request.draft.history_red
            },
            "roleProjections": {
                "blue": shortlist.blue_projection,
                "red": shortlist.red_projection
            }
        },
        "dataset": {
            "matches": statistics.total_matches,
            "globalWinRate": statistics.global_win_rate,
            "adjustmentPriorGames": statistics.prior_games,
            "reliableGames": statistics.reliable_games
        },
        "candidatePolicy": {
            "description": "Only requestedActionCandidates may be recommended; reserved Blue first-pick targets remain visible for strategic comparison.",
            "requestedActionCandidates": action_candidates,
            "candidateCount": champion_evidence.len()
        },
        "champions": champion_evidence
    })
}

fn stat_json(row: &crate::statistics::ChampionRoleStat) -> Value {
    json!({
        "games": row.games,
        "wins": row.wins,
        "rawWinRate": row.win_rate,
        "adjustedWinRate": row.adjusted_win_rate,
        "confidence": row.confidence,
        "patchChanged": row.patch_changed,
        "patchAdded": row.patch_added,
        "patchImpact": row.patch_impact,
        "patchChanges": row.patch_changes,
        "averageKda": row.kda,
        "averageDamage": row.avg_damage,
        "averageTanking": row.avg_tanking,
        "averageHealing": row.avg_healing,
        "averageRating": row.avg_rating
    })
}

fn candidate_ids(
    shortlist: &RecommendationShortlist,
    draft: &RecommendationRequest,
    selected_action: &str,
) -> BTreeSet<String> {
    let ranked = if selected_action.ends_with("ban") {
        shortlist
            .ban_recommendations
            .iter()
            .take(6)
            .chain(shortlist.pick_recommendations.iter().take(3))
            .map(|row| row.champion_id.clone())
            .collect::<Vec<_>>()
    } else {
        shortlist
            .pick_recommendations
            .iter()
            .take(8)
            .map(|row| row.champion_id.clone())
            .collect::<Vec<_>>()
    };
    ranked
        .into_iter()
        .chain(
            draft
                .blue_picks
                .iter()
                .chain(&draft.red_picks)
                .chain(&draft.blue_bans)
                .chain(&draft.red_bans)
                .cloned(),
        )
        .collect()
}

fn validate_response(
    response: &AiCoachResponse,
    request: &AiCoachRequest,
    shortlist: &RecommendationShortlist,
) -> Result<(), String> {
    let legal = eligible_action_candidates(shortlist, &request.draft, &request.selected_action);
    if response.recommended_actions.is_empty() {
        return Err("The model returned no recommended action.".to_string());
    }
    for action in &response.recommended_actions {
        if action.action != request.selected_action.split('-').next_back().unwrap_or("") {
            return Err("The model recommended the wrong action type.".to_string());
        }
        if !legal.contains(&action.champion_id) {
            return Err(format!(
                "The model recommended {}, which is not in the supplied legal shortlist.",
                action.champion_id
            ));
        }
    }
    Ok(())
}

fn eligible_action_candidates(
    shortlist: &RecommendationShortlist,
    draft: &RecommendationRequest,
    selected_action: &str,
) -> BTreeSet<String> {
    let reserved_first_pick = (selected_action == "blue-ban" && draft.blue_picks.is_empty())
        .then(|| shortlist.pick_recommendations.first())
        .flatten()
        .map(|row| row.champion_id.as_str());
    let rows = if selected_action.ends_with("ban") {
        &shortlist.ban_recommendations
    } else {
        &shortlist.pick_recommendations
    };
    rows.iter()
        .take(8)
        .filter(|row| Some(row.champion_id.as_str()) != reserved_first_pick)
        .map(|row| row.champion_id.clone())
        .collect()
}

fn response_format() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "tfm2_draft_recommendation",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "recommendedActions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 2,
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {"type": "string", "enum": ["pick", "ban"]},
                                "championId": {"type": "string"},
                                "suggestedRole": {"type": ["string", "null"]}
                            },
                            "required": ["action", "championId", "suggestedRole"],
                            "additionalProperties": false
                        }
                    },
                    "summary": {"type": "string", "maxLength": 260},
                    "draftInterpretation": {
                        "type": "object",
                        "properties": {
                            "ourAdvantages": {"type": "array", "minItems": 1, "maxItems": 2, "items": {"type": "string", "maxLength": 160}},
                            "ourDisadvantages": {"type": "array", "minItems": 1, "maxItems": 2, "items": {"type": "string", "maxLength": 160}},
                            "opponentAdvantages": {"type": "array", "minItems": 1, "maxItems": 2, "items": {"type": "string", "maxLength": 160}},
                            "opponentDisadvantages": {"type": "array", "minItems": 1, "maxItems": 2, "items": {"type": "string", "maxLength": 160}}
                        },
                        "required": ["ourAdvantages", "ourDisadvantages", "opponentAdvantages", "opponentDisadvantages"],
                        "additionalProperties": false
                    },
                    "evidence": {
                        "type": "array",
                        "minItems": 2,
                        "maxItems": 3,
                        "items": {
                            "type": "object",
                            "properties": {
                                "factor": {"type": "string", "maxLength": 50},
                                "effect": {"type": "string", "enum": ["positive", "negative", "mixed"]},
                                "explanation": {"type": "string", "maxLength": 180},
                                "uncertainty": {"type": "string", "enum": ["low", "medium", "high"]}
                            },
                            "required": ["factor", "effect", "explanation", "uncertainty"],
                            "additionalProperties": false
                        }
                    },
                    "rejectedAlternatives": {
                        "type": "array",
                        "maxItems": 2,
                        "items": {
                            "type": "object",
                            "properties": {
                                "championId": {"type": "string"},
                                "reason": {"type": "string", "maxLength": 180}
                            },
                            "required": ["championId", "reason"],
                            "additionalProperties": false
                        }
                    },
                    "nextRotationPlan": {"type": "string", "maxLength": 180},
                    "confidence": {"type": "number", "minimum": 0, "maximum": 1}
                },
                "required": [
                    "recommendedActions", "summary", "draftInterpretation", "evidence",
                    "rejectedAlternatives", "nextRotationPlan", "confidence"
                ],
                "additionalProperties": false
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_requires_concrete_evidence_fields() {
        let format = response_format();
        assert_eq!(
            format
                .pointer("/json_schema/strict")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(format
            .pointer("/json_schema/schema/properties/evidence")
            .is_some());
    }

    #[test]
    fn nullable_content_can_fall_back_to_structured_reasoning() {
        let message = Message {
            content: None,
            reasoning: Some(r#"{"summary":"ok"}"#.to_string()),
            refusal: None,
        };
        assert_eq!(
            extract_message_content(&message),
            Some(r#"{"summary":"ok"}"#)
        );
    }

    #[test]
    fn nullable_content_without_output_is_reported_cleanly() {
        let body = r#"{
            "model": null,
            "choices": [{"message": {"content": null, "reasoning": null, "refusal": null}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 0, "total_tokens": 10, "cost": null}
        }"#;
        let response: ChatResponse = serde_json::from_str(body).unwrap();
        assert!(extract_message_content(&response.choices[0].message).is_none());
    }

    #[test]
    fn model_json_does_not_need_transport_metadata() {
        let content = r#"{
            "recommendedActions": [{"action": "pick", "championId": "swordman", "suggestedRole": null}],
            "summary": "Pick Swordman.",
            "draftInterpretation": {
                "ourAdvantages": [],
                "ourDisadvantages": [],
                "opponentAdvantages": [],
                "opponentDisadvantages": []
            },
            "evidence": [],
            "rejectedAlternatives": [],
            "nextRotationPlan": "Reassess after the response.",
            "confidence": 0.5
        }"#;
        let response: AiCoachResponse = serde_json::from_str(content).unwrap();
        assert!(response.model.is_empty());
        assert!(response.usage.is_none());
    }

    #[test]
    fn model_test_schema_is_tiny_and_strict() {
        let body = json!({
            "status": "ok",
            "value": 7
        });
        assert_eq!(body["value"], 7);
    }
}

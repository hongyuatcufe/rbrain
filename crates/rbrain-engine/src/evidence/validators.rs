use rbrain_core::error::{BrainError, Result};
use sqlx::{Row, SqlitePool};

use super::actions::SuggestedAction;
use super::result::ValidatorResult;

fn db_err<E: std::fmt::Display>(e: E) -> BrainError {
    BrainError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        e.to_string(),
    ))
}

pub async fn research_run_has_input(
    pool: &SqlitePool,
    run_slug: &str,
) -> Result<ValidatorResult> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM links l
         JOIN pages p ON p.slug = l.target_slug
         WHERE l.source_slug = ?
           AND l.edge_type IN ('uses_dataset', 'uses_corpus', 'uses_method', 'cites', 'references')
           AND p.page_type IN ('dataset', 'literature_corpus', 'citation_record', 'source', 'method_note', 'research_memo')",
    )
    .bind(run_slug)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    Ok(if count > 0 {
        ValidatorResult::pass("research_run_has_input")
    } else {
        ValidatorResult::fail(
            "research_run_has_input",
            "research_run has no registered input pages",
        )
        .with_affected([run_slug.to_string()])
        .with_actions([SuggestedAction::RegisterInput { hint: None }])
    })
}

pub async fn produced_artifact_exists(
    pool: &SqlitePool,
    run_slug: &str,
) -> Result<ValidatorResult> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM links l
         JOIN pages p ON p.slug = l.target_slug
         WHERE l.source_slug = ?
           AND l.edge_type = 'produces'
           AND p.page_type = 'artifact'",
    )
    .bind(run_slug)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    Ok(if count > 0 {
        ValidatorResult::pass("produced_artifact_exists")
    } else {
        ValidatorResult::warn(
            "produced_artifact_exists",
            "research_run has not produced any artifact yet",
        )
        .with_affected([run_slug.to_string()])
        .with_actions([SuggestedAction::RegisterArtifact { hint: None }])
    })
}

pub async fn artifact_hash_present(
    pool: &SqlitePool,
    run_slug: &str,
) -> Result<ValidatorResult> {
    let rows = sqlx::query(
        "SELECT DISTINCT p.slug, p.frontmatter
         FROM links l
         JOIN pages p ON p.slug = l.target_slug
         WHERE l.source_slug = ?
           AND l.edge_type = 'produces'
           AND p.page_type = 'artifact'",
    )
    .bind(run_slug)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    if rows.is_empty() {
        return Ok(ValidatorResult::warn(
            "artifact_hash_present",
            "no artifacts linked from research_run yet",
        ));
    }

    let mut missing = Vec::new();
    for row in &rows {
        let slug: String = row.try_get("slug").map_err(db_err)?;
        let fm: String = row.try_get("frontmatter").map_err(db_err)?;
        let fm: serde_json::Value = serde_json::from_str(&fm).unwrap_or(serde_json::Value::Null);
        let hash = fm.get("hash").and_then(|v| v.as_str()).unwrap_or("");
        if hash.trim().is_empty() {
            missing.push(slug);
        }
    }

    Ok(if missing.is_empty() {
        ValidatorResult::pass("artifact_hash_present")
    } else {
        let actions = missing
            .iter()
            .map(|slug| SuggestedAction::RefineArtifact { slug: slug.clone() })
            .collect::<Vec<_>>();
        ValidatorResult::warn(
            "artifact_hash_present",
            format!("{} artifact(s) missing hash", missing.len()),
        )
        .with_affected(missing)
        .with_actions(actions)
    })
}

pub async fn finding_has_supporting_evidence(
    pool: &SqlitePool,
    run_slug: &str,
) -> Result<ValidatorResult> {
    let rows = sqlx::query(
        "SELECT DISTINCT p.slug, p.frontmatter FROM pages p
         JOIN links l ON l.target_slug = p.slug
         WHERE p.page_type = 'finding'
           AND l.source_slug = ?
           AND l.edge_type = 'produces'",
    )
    .bind(run_slug)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    if rows.is_empty() {
        return Ok(ValidatorResult::warn(
            "finding_has_supporting_evidence",
            "no findings linked to research_run yet",
        ));
    }

    let mut draft_missing = Vec::new();
    let mut claim_missing = Vec::new();

    for row in &rows {
        let slug: String = row.try_get("slug").map_err(db_err)?;
        let fm: String = row.try_get("frontmatter").map_err(db_err)?;
        let fm: serde_json::Value = serde_json::from_str(&fm).unwrap_or(serde_json::Value::Null);
        let status = fm.get("status").and_then(|v| v.as_str()).unwrap_or("claim");

        let support_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM links l
             JOIN pages p ON p.slug = l.target_slug
             WHERE l.source_slug = ?
               AND l.edge_type IN ('supports', 'cites')
               AND p.page_type IN ('artifact', 'dataset', 'literature_corpus', 'citation_record', 'source', 'note', 'paper', 'book')",
        )
        .bind(&slug)
        .fetch_one(pool)
        .await
        .map_err(db_err)?;

        if support_count == 0 {
            if status == "draft" {
                draft_missing.push(slug);
            } else {
                claim_missing.push(slug);
            }
        }
    }

    if !claim_missing.is_empty() {
        let actions = claim_missing
            .iter()
            .map(|slug| SuggestedAction::LinkEvidence {
                from: slug.clone(),
                to: String::new(),
                link_type: "supports".to_string(),
            })
            .collect::<Vec<_>>();
        return Ok(ValidatorResult::fail(
            "finding_has_supporting_evidence",
            format!("{} finding(s) without supporting evidence", claim_missing.len()),
        )
        .with_affected(claim_missing)
        .with_actions(actions));
    }

    if !draft_missing.is_empty() {
        return Ok(ValidatorResult::warn(
            "finding_has_supporting_evidence",
            format!(
                "{} draft finding(s) without supporting evidence",
                draft_missing.len()
            ),
        )
        .with_affected(draft_missing));
    }

    Ok(ValidatorResult::pass("finding_has_supporting_evidence"))
}

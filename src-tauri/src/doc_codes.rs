use crate::error::{AppError, AppResult};

/// Map category key → business type code (MOD, PL, MC, ANT, …).
pub fn type_code_for_category(category_key: &str) -> Option<&'static str> {
    match category_key {
        "modelos" => Some("MOD"),
        "planos" => Some("PL"),
        "memorias" => Some("MC"),
        "antecedentes" => Some("ANT"),
        "admin" => Some("ADM"),
        "otros" => Some("OTR"),
        "transmisiones" => None, // ENV/REC use next_transmission_code
        _ => None,
    }
}

/// Next transmission business code: `PROYECTO-ENV-NNN` or `PROYECTO-REC-NNN`.
pub fn next_transmission_code(
    project_code: &str,
    kind: &str, // ENV | REC
    existing_codes: &[String],
) -> AppResult<String> {
    let kind = kind.to_ascii_uppercase();
    if kind != "ENV" && kind != "REC" {
        return Err(AppError::msg("Tipo de transmisión debe ser ENV o REC"));
    }
    next_document_code(project_code, &kind, existing_codes)
}

/// Next `PROYECTO-TIPO-CORRELATIVO` (e.g. 2601-MOD-003).
pub fn next_document_code(
    project_code: &str,
    type_code: &str,
    existing_codes: &[String],
) -> AppResult<String> {
    if project_code.trim().is_empty() || type_code.trim().is_empty() {
        return Err(AppError::msg("Código de proyecto o tipo inválido"));
    }
    let prefix = format!("{project_code}-{type_code}-");
    let mut max_corr: i32 = 0;
    for code in existing_codes {
        if let Some(rest) = code.strip_prefix(&prefix) {
            if let Ok(n) = rest.parse::<i32>() {
                max_corr = max_corr.max(n);
            }
        }
    }
    let next = max_corr + 1;
    if next > 9999 {
        return Err(AppError::msg(
            "Se superó el límite de correlativos para este tipo de documento",
        ));
    }
    Ok(format!("{prefix}{next:03}"))
}

pub fn version_label(n: i64) -> String {
    format!("v{n:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_mod_code() {
        let code = next_document_code("2601", "MOD", &[]).unwrap();
        assert_eq!(code, "2601-MOD-001");
    }

    #[test]
    fn increments_per_type() {
        let existing = vec![
            "2601-MOD-001".into(),
            "2601-MOD-002".into(),
            "2601-PL-001".into(),
        ];
        assert_eq!(
            next_document_code("2601", "MOD", &existing).unwrap(),
            "2601-MOD-003"
        );
        assert_eq!(
            next_document_code("2601", "PL", &existing).unwrap(),
            "2601-PL-002"
        );
    }

    #[test]
    fn category_mapping() {
        assert_eq!(type_code_for_category("modelos"), Some("MOD"));
        assert_eq!(type_code_for_category("planos"), Some("PL"));
        assert_eq!(type_code_for_category("memorias"), Some("MC"));
        assert_eq!(type_code_for_category("antecedentes"), Some("ANT"));
    }

    #[test]
    fn env_rec_codes() {
        assert_eq!(
            next_transmission_code("2601", "ENV", &[]).unwrap(),
            "2601-ENV-001"
        );
        assert_eq!(
            next_transmission_code("2601", "REC", &["2601-ENV-001".into()]).unwrap(),
            "2601-REC-001"
        );
        assert_eq!(
            next_transmission_code("2601", "ENV", &["2601-ENV-001".into(), "2601-ENV-002".into()])
                .unwrap(),
            "2601-ENV-003"
        );
    }
}

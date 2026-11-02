use crate::error::{AppError, AppResult};

/// Generate next AANN project code for a given two-digit year prefix.
/// Existing codes like 2601, 2602 → next is 2603.
/// If correlative would exceed 99, returns an error asking for expansion.
pub fn next_project_code(year_two_digits: u32, existing_codes: &[String]) -> AppResult<String> {
    if !(0..=99).contains(&year_two_digits) {
        return Err(AppError::msg("Año inválido para código AANN"));
    }

    let prefix = format!("{year_two_digits:02}");
    let mut max_corr: i32 = 0;
    for code in existing_codes {
        if code.len() == 4 && code.starts_with(&prefix) {
            if let Ok(corr) = code[2..].parse::<i32>() {
                max_corr = max_corr.max(corr);
            }
        }
    }

    let next = max_corr + 1;
    if next > 99 {
        return Err(AppError::msg(
            "Se superó el límite de 99 proyectos para este año. Se requiere ampliación de codificación.",
        ));
    }

    Ok(format!("{prefix}{next:02}"))
}

pub fn current_year_two_digits() -> u32 {
    use chrono::{Datelike, Local};
    (Local::now().year() % 100) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_code_of_year() {
        let code = next_project_code(26, &[]).unwrap();
        assert_eq!(code, "2601");
    }

    #[test]
    fn increments_existing() {
        let existing = vec!["2601".into(), "2603".into(), "2501".into()];
        let code = next_project_code(26, &existing).unwrap();
        assert_eq!(code, "2604");
    }

    #[test]
    fn rejects_over_99() {
        let existing: Vec<String> = (1..=99).map(|n| format!("26{n:02}")).collect();
        let err = next_project_code(26, &existing).unwrap_err();
        assert!(err.to_string().contains("ampliación"));
    }
}

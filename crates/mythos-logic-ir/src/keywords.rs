//! The keywords of IEC 61131-3, which no language's program may use as a name.
//!
//! IEC 61131-3 ed. 3 §6.1.3: keywords "shall not be used for any other purpose, for example,
//! variable names". Ed. 3 has no table of them (the rule is all §6.1.3 gives), so the list
//! below is taken from where the standard defines its syntax: the terminal symbols of Annex A
//! (normative) that are words standing as a syntactic element of their own, in the productions
//! of the common elements, of Structured Text, of the textual form of SFC, of configurations
//! and of namespaces; and the keyword column of Table 10 for the elementary data types (which
//! adds LTIME_OF_DAY and LDATE_AND_TIME to the type names of Annex A). Figure 7 calls the
//! words of the variable sections and their qualifiers keywords as well.
//!
//! It lives beside the IR because it is the same for every language: a name is refused in ST
//! for the reason it will be refused in LD, FBD and SFC.
//!
//! Words that Annex A writes in quotes and that are **not** here, and why:
//!
//! - *Parts of a token.* The letters of a direct variable after `%` (I, Q, M, X, B, W, D, L),
//!   the prefixes T, LT, D and LD that occur only before `#`, and the units of a duration.
//! - *The operators of IL* (Table 68: LD, ST, S, R, JMP, CAL, RET and the rest, with their
//!   modifiers) and the inputs Annex A lists beside them (S1, R1, CLK, CU, CD, PV, IN, PT).
//!   They are what stands in the operator field of an IL instruction. The standard names
//!   variables with them outside IL: `LD` and `R` are inputs of its counters and are read in
//!   their Structured Text bodies (Table 45), `S` and `R` are inputs of its bistables
//!   (Table 43), and Table 39 declares `VAR R: REAL; END_VAR`. A word the standard declares
//!   as a variable is not one §6.1.3 forbids as a variable. AND, OR, XOR, NOT and MOD are
//!   here because they are operators of Structured Text. (IL itself is not built: ADR-013
//!   §2.1.)
//! - *The action qualifiers of SFC* (Table 59: N, R, S, L, D, P, SD, DS, SL): the values of one
//!   place in an action association. R and S are the variables just named; N, L and P are
//!   formal parameters of standard functions (Table 30: `SHL(IN := …, N := …)`; Table 34:
//!   `MID(IN := …, L := …, P := …)`).
//! - *SINGLE, INTERVAL and PRIORITY*: Table 63 draws them as the inputs of a TASK, given as
//!   `name := value` like any input.
//! - *The names of standard functions and function blocks* (ABS, MAX, TON, CTU, …). Annex A
//!   marks both of its lists "incomplete list": they are names of library elements, not
//!   syntax. A front-end refuses a second declaration of such a name where it would be
//!   ambiguous, as it does for any name declared twice.

/// Every keyword, in upper case, sorted.
pub const KEYWORDS: [&str; 127] = [
    "ABSTRACT",
    "ACTION",
    "AND",
    "ARRAY",
    "AT",
    "BOOL",
    "BY",
    "BYTE",
    "CASE",
    "CHAR",
    "CLASS",
    "CONFIGURATION",
    "CONSTANT",
    "CONTINUE",
    "DATE",
    "DATE_AND_TIME",
    "DINT",
    "DO",
    "DT",
    "DWORD",
    "ELSE",
    "ELSIF",
    "END_ACTION",
    "END_CASE",
    "END_CLASS",
    "END_CONFIGURATION",
    "END_FOR",
    "END_FUNCTION",
    "END_FUNCTION_BLOCK",
    "END_IF",
    "END_INTERFACE",
    "END_METHOD",
    "END_NAMESPACE",
    "END_PROGRAM",
    "END_REPEAT",
    "END_RESOURCE",
    "END_STEP",
    "END_STRUCT",
    "END_TRANSITION",
    "END_TYPE",
    "END_VAR",
    "END_WHILE",
    "EXIT",
    "EXTENDS",
    "FALSE",
    "FINAL",
    "FOR",
    "FROM",
    "FUNCTION",
    "FUNCTION_BLOCK",
    "F_EDGE",
    "IF",
    "IMPLEMENTS",
    "INITIAL_STEP",
    "INT",
    "INTERFACE",
    "INTERNAL",
    "LDATE",
    "LDATE_AND_TIME",
    "LDT",
    "LINT",
    "LREAL",
    "LTIME",
    "LTIME_OF_DAY",
    "LTOD",
    "LWORD",
    "METHOD",
    "MOD",
    "NAMESPACE",
    "NON_RETAIN",
    "NOT",
    "NULL",
    "OF",
    "ON",
    "OR",
    "OVERLAP",
    "OVERRIDE",
    "PRIVATE",
    "PROGRAM",
    "PROTECTED",
    "PUBLIC",
    "READ_ONLY",
    "READ_WRITE",
    "REAL",
    "REF",
    "REF_TO",
    "REPEAT",
    "RESOURCE",
    "RETAIN",
    "RETURN",
    "R_EDGE",
    "SINT",
    "STEP",
    "STRING",
    "STRUCT",
    "SUPER",
    "TASK",
    "THEN",
    "THIS",
    "TIME",
    "TIME_OF_DAY",
    "TO",
    "TOD",
    "TRANSITION",
    "TRUE",
    "TYPE",
    "UDINT",
    "UINT",
    "ULINT",
    "UNTIL",
    "USING",
    "USINT",
    "VAR",
    "VAR_ACCESS",
    "VAR_CONFIG",
    "VAR_EXTERNAL",
    "VAR_GLOBAL",
    "VAR_INPUT",
    "VAR_IN_OUT",
    "VAR_OUTPUT",
    "VAR_TEMP",
    "WCHAR",
    "WHILE",
    "WITH",
    "WORD",
    "WSTRING",
    "XOR",
];

/// The keywords that name an elementary data type (Table 10): the keywords a program writes
/// where a type is named.
pub const TYPE_KEYWORDS: [&str; 31] = [
    "BOOL",
    "BYTE",
    "CHAR",
    "DATE",
    "DATE_AND_TIME",
    "DINT",
    "DT",
    "DWORD",
    "INT",
    "LDATE",
    "LDATE_AND_TIME",
    "LDT",
    "LINT",
    "LREAL",
    "LTIME",
    "LTIME_OF_DAY",
    "LTOD",
    "LWORD",
    "REAL",
    "SINT",
    "STRING",
    "TIME",
    "TIME_OF_DAY",
    "TOD",
    "UDINT",
    "UINT",
    "ULINT",
    "USINT",
    "WCHAR",
    "WORD",
    "WSTRING",
];

/// Whether `name` is a keyword of the standard, whatever its case (§6.1.3: "the case of
/// characters shall not be significant in keywords").
pub fn is_keyword(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    KEYWORDS.binary_search(&upper.as_str()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_and_has_no_word_twice_so_the_search_finds_every_one() {
        for pair in KEYWORDS.windows(2) {
            assert!(pair[0] < pair[1], "{} before {}", pair[0], pair[1]);
        }
        for word in KEYWORDS {
            assert!(is_keyword(word), "{word}");
            assert!(
                is_keyword(&word.to_ascii_lowercase()),
                "{word} in lower case"
            );
        }
    }

    /// The list again, by where the standard gives each word: a word dropped from
    /// [`KEYWORDS`], or one added to it, is found here.
    #[test]
    fn the_keywords_are_the_words_of_annex_a_and_table_10_production_by_production() {
        let by_production: [(&str, &[&str]); 9] = [
            ("Table 5: Bool_Literal", &["FALSE", "TRUE"]),
            (
                "Table 10: the keyword of each elementary data type",
                &TYPE_KEYWORDS,
            ),
            (
                "Tables 11 and 12: declaration of data types, references",
                &[
                    "TYPE",
                    "END_TYPE",
                    "ARRAY",
                    "OF",
                    "STRUCT",
                    "OVERLAP",
                    "END_STRUCT",
                    "REF_TO",
                    "REF",
                    "NULL",
                ],
            ),
            (
                "Tables 13 and 14, Figure 7: declaration of variables",
                &[
                    "VAR",
                    "VAR_INPUT",
                    "VAR_OUTPUT",
                    "VAR_IN_OUT",
                    "VAR_TEMP",
                    "VAR_EXTERNAL",
                    "VAR_GLOBAL",
                    "VAR_ACCESS",
                    "VAR_CONFIG",
                    "END_VAR",
                    "CONSTANT",
                    "RETAIN",
                    "NON_RETAIN",
                    "R_EDGE",
                    "F_EDGE",
                    "AT",
                    "THIS",
                ],
            ),
            (
                "Tables 19, 40, 47, 48: functions, function blocks, programs, classes",
                &[
                    "FUNCTION",
                    "END_FUNCTION",
                    "FUNCTION_BLOCK",
                    "END_FUNCTION_BLOCK",
                    "PROGRAM",
                    "END_PROGRAM",
                    "METHOD",
                    "END_METHOD",
                    "CLASS",
                    "END_CLASS",
                    "INTERFACE",
                    "END_INTERFACE",
                    "FINAL",
                    "ABSTRACT",
                    "OVERRIDE",
                    "EXTENDS",
                    "IMPLEMENTS",
                    "PUBLIC",
                    "PROTECTED",
                    "PRIVATE",
                    "INTERNAL",
                    "SUPER",
                ],
            ),
            (
                "Tables 54 to 61: SFC",
                &[
                    "INITIAL_STEP",
                    "STEP",
                    "END_STEP",
                    "TRANSITION",
                    "FROM",
                    "TO",
                    "END_TRANSITION",
                    "ACTION",
                    "END_ACTION",
                ],
            ),
            (
                "Table 62: configurations and resources",
                &[
                    "CONFIGURATION",
                    "END_CONFIGURATION",
                    "RESOURCE",
                    "ON",
                    "END_RESOURCE",
                    "TASK",
                    "WITH",
                    "READ_WRITE",
                    "READ_ONLY",
                ],
            ),
            (
                "Table 64: namespaces",
                &["NAMESPACE", "END_NAMESPACE", "USING"],
            ),
            (
                "Tables 71 and 72: Structured Text",
                &[
                    "OR",
                    "XOR",
                    "AND",
                    "MOD",
                    "NOT",
                    "IF",
                    "THEN",
                    "ELSIF",
                    "ELSE",
                    "END_IF",
                    "CASE",
                    "END_CASE",
                    "FOR",
                    "BY",
                    "DO",
                    "END_FOR",
                    "WHILE",
                    "END_WHILE",
                    "REPEAT",
                    "UNTIL",
                    "END_REPEAT",
                    "EXIT",
                    "CONTINUE",
                    "RETURN",
                ],
            ),
        ];
        let mut all: Vec<&str> = Vec::new();
        for (production, words) in by_production {
            for word in words {
                assert!(is_keyword(word), "{word} ({production}) is not in the list");
                assert!(!all.contains(word), "{word} ({production}) is given twice");
                all.push(word);
            }
        }
        all.sort_unstable();
        assert_eq!(all, KEYWORDS, "the list has a word no production gives");
    }

    #[test]
    fn every_type_keyword_is_a_keyword() {
        for word in TYPE_KEYWORDS {
            assert!(is_keyword(word), "{word}");
        }
    }

    #[test]
    fn the_names_the_standard_gives_its_own_variables_are_not_keywords() {
        // Tables 43, 45 and 46: inputs and outputs of the standard function blocks. Table 39:
        // `VAR R: REAL; END_VAR`. Tables 30 and 34: formal parameters of standard functions.
        // §6.6.1.5 declares EN and ENO as variables.
        for name in [
            "S1", "R", "Q1", "S", "R1", "CLK", "Q", "CU", "CD", "LD", "PV", "CV", "QU", "QD", "IN",
            "PT", "ET", "N", "L", "P", "EN", "ENO",
        ] {
            assert!(!is_keyword(name), "{name}");
        }
        // And the standard function blocks themselves are names, not keywords.
        for name in ["TON", "CTU", "R_TRIG", "ABS", "MAX"] {
            assert!(!is_keyword(name), "{name}");
        }
    }
}

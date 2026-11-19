(* The bistables of IEC 61131-3 ed. 3 §6.6.3.5.2, Table 43: each the function block body the
   table draws, written in Structured Text. Q1 starts FALSE (footnote a). The long input
   names of features 1b and 2b (SET1, RESET; SET, RESET1) are not given: a block here has one
   name for each input. *)

(* Table 43 feature 1a: set dominant. *)
FUNCTION_BLOCK SR
VAR_INPUT
    S1 : BOOL;
    R : BOOL;
END_VAR
VAR_OUTPUT
    Q1 : BOOL;
END_VAR
    Q1 := S1 OR (NOT R AND Q1);
END_FUNCTION_BLOCK

(* Table 43 feature 2a: reset dominant. *)
FUNCTION_BLOCK RS
VAR_INPUT
    S : BOOL;
    R1 : BOOL;
END_VAR
VAR_OUTPUT
    Q1 : BOOL;
END_VAR
    Q1 := NOT R1 AND (S OR Q1);
END_FUNCTION_BLOCK

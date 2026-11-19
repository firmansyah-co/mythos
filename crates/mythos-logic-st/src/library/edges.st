(* The edge detectors of IEC 61131-3 ed. 3 §6.6.3.5.3: the definitions of Table 44, as the
   table writes them. M starts FALSE, so an F_TRIG whose CLK is FALSE gives Q on its first
   call after a cold restart, as the table's note says. *)

(* Table 44 feature 1. *)
FUNCTION_BLOCK R_TRIG
VAR_INPUT
    CLK : BOOL;
END_VAR
VAR_OUTPUT
    Q : BOOL;
END_VAR
VAR
    M : BOOL;
END_VAR
    Q := CLK AND NOT M;
    M := CLK;
END_FUNCTION_BLOCK

(* Table 44 feature 2. *)
FUNCTION_BLOCK F_TRIG
VAR_INPUT
    CLK : BOOL;
END_VAR
VAR_OUTPUT
    Q : BOOL;
END_VAR
VAR
    M : BOOL;
END_VAR
    Q := NOT CLK AND NOT M;
    M := NOT CLK;
END_FUNCTION_BLOCK

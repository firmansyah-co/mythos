(* The counters of IEC 61131-3 ed. 3 §6.6.3.5.4: the function block bodies of Table 45, in
   Structured Text.

   CU and CD are declared BOOL R_EDGE in the table: the block counts a rising edge, not a
   level. §6.6.3.2 rule 13 has R_EDGE detect the edge inside the block; here each edge has its
   memory (CU_M, CD_M) as a variable of the block, so that it is retained with the instance
   (§6.5.6.2) as the count is. PVmax and PVmin are the limits of CV's type (the table's note
   leaves them to the implementer): a counter counts on past PV, up to them.

   CTU, CTD and CTUD are those of INT (features 1a, 2a, 3a). Of the typed variants, those of
   INT, DINT and LINT are here; UDINT and ULINT (1d, 1e, 2d, 2e, 3d, 3e) are not, because this
   compiler has no unsigned integer types. *)

(* Table 45 feature 1a (CTU, the same as CTU_INT). *)
FUNCTION_BLOCK CTU
VAR_INPUT
    CU : BOOL;
    R : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : INT;
END_VAR
VAR
    CU_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF CU AND NOT CU_M AND (CV < 32767) THEN
        CV := CV + 1;
    END_IF;
    CU_M := CU;
    Q := CV >= PV;
END_FUNCTION_BLOCK

(* Table 45 feature 1a. *)
FUNCTION_BLOCK CTU_INT
VAR_INPUT
    CU : BOOL;
    R : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : INT;
END_VAR
VAR
    CU_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF CU AND NOT CU_M AND (CV < 32767) THEN
        CV := CV + 1;
    END_IF;
    CU_M := CU;
    Q := CV >= PV;
END_FUNCTION_BLOCK

(* Table 45 feature 1b. *)
FUNCTION_BLOCK CTU_DINT
VAR_INPUT
    CU : BOOL;
    R : BOOL;
    PV : DINT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : DINT;
END_VAR
VAR
    CU_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF CU AND NOT CU_M AND (CV < 2147483647) THEN
        CV := CV + 1;
    END_IF;
    CU_M := CU;
    Q := CV >= PV;
END_FUNCTION_BLOCK

(* Table 45 feature 1c. *)
FUNCTION_BLOCK CTU_LINT
VAR_INPUT
    CU : BOOL;
    R : BOOL;
    PV : LINT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : LINT;
END_VAR
VAR
    CU_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF CU AND NOT CU_M AND (CV < 9223372036854775807) THEN
        CV := CV + 1;
    END_IF;
    CU_M := CU;
    Q := CV >= PV;
END_FUNCTION_BLOCK

(* Table 45 feature 2a (CTD, the same as CTD_INT). *)
FUNCTION_BLOCK CTD
VAR_INPUT
    CD : BOOL;
    LD : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : INT;
END_VAR
VAR
    CD_M : BOOL;
END_VAR
    IF LD THEN
        CV := PV;
    ELSIF CD AND NOT CD_M AND (CV > -32768) THEN
        CV := CV - 1;
    END_IF;
    CD_M := CD;
    Q := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 2a. *)
FUNCTION_BLOCK CTD_INT
VAR_INPUT
    CD : BOOL;
    LD : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : INT;
END_VAR
VAR
    CD_M : BOOL;
END_VAR
    IF LD THEN
        CV := PV;
    ELSIF CD AND NOT CD_M AND (CV > -32768) THEN
        CV := CV - 1;
    END_IF;
    CD_M := CD;
    Q := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 2b. *)
FUNCTION_BLOCK CTD_DINT
VAR_INPUT
    CD : BOOL;
    LD : BOOL;
    PV : DINT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : DINT;
END_VAR
VAR
    CD_M : BOOL;
END_VAR
    IF LD THEN
        CV := PV;
    ELSIF CD AND NOT CD_M AND (CV > -2147483648) THEN
        CV := CV - 1;
    END_IF;
    CD_M := CD;
    Q := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 2c. *)
FUNCTION_BLOCK CTD_LINT
VAR_INPUT
    CD : BOOL;
    LD : BOOL;
    PV : LINT;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    CV : LINT;
END_VAR
VAR
    CD_M : BOOL;
END_VAR
    IF LD THEN
        CV := PV;
    ELSIF CD AND NOT CD_M AND (CV > -9223372036854775808) THEN
        CV := CV - 1;
    END_IF;
    CD_M := CD;
    Q := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 3a (CTUD, the same as CTUD_INT). *)
FUNCTION_BLOCK CTUD
VAR_INPUT
    CU : BOOL;
    CD : BOOL;
    R : BOOL;
    LD : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    QU : BOOL;
    QD : BOOL;
    CV : INT;
END_VAR
VAR
    CU_M : BOOL;
    CD_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF LD THEN
        CV := PV;
    ELSE
        IF NOT ((CU AND NOT CU_M) AND (CD AND NOT CD_M)) THEN
            IF CU AND NOT CU_M AND (CV < 32767) THEN
                CV := CV + 1;
            ELSIF CD AND NOT CD_M AND (CV > -32768) THEN
                CV := CV - 1;
            END_IF;
        END_IF;
    END_IF;
    CU_M := CU;
    CD_M := CD;
    QU := CV >= PV;
    QD := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 3a. *)
FUNCTION_BLOCK CTUD_INT
VAR_INPUT
    CU : BOOL;
    CD : BOOL;
    R : BOOL;
    LD : BOOL;
    PV : INT;
END_VAR
VAR_OUTPUT
    QU : BOOL;
    QD : BOOL;
    CV : INT;
END_VAR
VAR
    CU_M : BOOL;
    CD_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF LD THEN
        CV := PV;
    ELSE
        IF NOT ((CU AND NOT CU_M) AND (CD AND NOT CD_M)) THEN
            IF CU AND NOT CU_M AND (CV < 32767) THEN
                CV := CV + 1;
            ELSIF CD AND NOT CD_M AND (CV > -32768) THEN
                CV := CV - 1;
            END_IF;
        END_IF;
    END_IF;
    CU_M := CU;
    CD_M := CD;
    QU := CV >= PV;
    QD := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 3b. *)
FUNCTION_BLOCK CTUD_DINT
VAR_INPUT
    CU : BOOL;
    CD : BOOL;
    R : BOOL;
    LD : BOOL;
    PV : DINT;
END_VAR
VAR_OUTPUT
    QU : BOOL;
    QD : BOOL;
    CV : DINT;
END_VAR
VAR
    CU_M : BOOL;
    CD_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF LD THEN
        CV := PV;
    ELSE
        IF NOT ((CU AND NOT CU_M) AND (CD AND NOT CD_M)) THEN
            IF CU AND NOT CU_M AND (CV < 2147483647) THEN
                CV := CV + 1;
            ELSIF CD AND NOT CD_M AND (CV > -2147483648) THEN
                CV := CV - 1;
            END_IF;
        END_IF;
    END_IF;
    CU_M := CU;
    CD_M := CD;
    QU := CV >= PV;
    QD := CV <= 0;
END_FUNCTION_BLOCK

(* Table 45 feature 3c. *)
FUNCTION_BLOCK CTUD_LINT
VAR_INPUT
    CU : BOOL;
    CD : BOOL;
    R : BOOL;
    LD : BOOL;
    PV : LINT;
END_VAR
VAR_OUTPUT
    QU : BOOL;
    QD : BOOL;
    CV : LINT;
END_VAR
VAR
    CU_M : BOOL;
    CD_M : BOOL;
END_VAR
    IF R THEN
        CV := 0;
    ELSIF LD THEN
        CV := PV;
    ELSE
        IF NOT ((CU AND NOT CU_M) AND (CD AND NOT CD_M)) THEN
            IF CU AND NOT CU_M AND (CV < 9223372036854775807) THEN
                CV := CV + 1;
            ELSIF CD AND NOT CD_M AND (CV > -9223372036854775808) THEN
                CV := CV - 1;
            END_IF;
        END_IF;
    END_IF;
    CU_M := CU;
    CD_M := CD;
    QU := CV >= PV;
    QD := CV <= 0;
END_FUNCTION_BLOCK

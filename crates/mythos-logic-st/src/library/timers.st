(* The timers of IEC 61131-3 ed. 3 §6.6.3.5.5, Table 46, as the timing diagrams of Figure 15
   draw them, Q and ET both.

   The clock. __NOW() is the scan clock: the time since the program was put in RUN, the same
   for the whole of one scan (an intrinsic, which only this library can name). START is when
   the timing began, on that clock; ET is CLOCK - START, kept between 0 and PT.

   A restart. The scan clock starts again from zero whenever the program is started, while a
   warm restart keeps a RETAIN instance's state, START included, which was taken on the clock
   before. So a clock below the one this instance saw at its previous call (CLOCK) means the
   program was started again: the timing carries on from the ET it had, START := now - ET, so
   ET is never negative and never more than PT. A restart is not seen when the instance is
   first called after it at a clock already past its previous call's: ET then grows by the
   clock less CLOCK, which is no more than the time since the restart.

   A difference that does not fit in a TIME (START more than 292 years before the clock) is
   past any PT.

   Table 46's note leaves a change of PT during the timing to the implementer: ET is always
   the time since START, limited by the PT of the call.

   TP_TIME, TON_TIME and TOF_TIME (features 1b, 2b, 3b) are TP, TON and TOF, of TIME. Their
   LTIME forms (1c, 2c, 3c) are not here: this compiler has no LTIME. *)

(* Table 46 feature 1a; Figure 15 a). A rising edge of IN starts a pulse of PT unless one
   runs; once it ends, ET holds PT while IN stays TRUE and is 0 once IN is FALSE. *)
FUNCTION_BLOCK TP
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
    IN_M : BOOL;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF NOT RUNNING AND IN AND NOT IN_M THEN
        RUNNING := TRUE;
        Q := TRUE;
        START := CLOCK;
    END_IF;
    IF RUNNING THEN
        ET := CLOCK - START;
        IF ET < T#0s OR ET > PT THEN
            ET := PT;
        END_IF;
        IF ET < T#0s THEN
            ET := T#0s;
        END_IF;
        IF ET >= PT THEN
            Q := FALSE;
            RUNNING := FALSE;
        END_IF;
    END_IF;
    IF NOT RUNNING AND NOT IN THEN
        ET := T#0s;
    END_IF;
    IN_M := IN;
END_FUNCTION_BLOCK

(* Table 46 feature 1b; Figure 15 a). A rising edge of IN starts a pulse of PT unless one
   runs; once it ends, ET holds PT while IN stays TRUE and is 0 once IN is FALSE. *)
FUNCTION_BLOCK TP_TIME
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
    IN_M : BOOL;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF NOT RUNNING AND IN AND NOT IN_M THEN
        RUNNING := TRUE;
        Q := TRUE;
        START := CLOCK;
    END_IF;
    IF RUNNING THEN
        ET := CLOCK - START;
        IF ET < T#0s OR ET > PT THEN
            ET := PT;
        END_IF;
        IF ET < T#0s THEN
            ET := T#0s;
        END_IF;
        IF ET >= PT THEN
            Q := FALSE;
            RUNNING := FALSE;
        END_IF;
    END_IF;
    IF NOT RUNNING AND NOT IN THEN
        ET := T#0s;
    END_IF;
    IN_M := IN;
END_FUNCTION_BLOCK

(* Table 46 feature 2a; Figure 15 b). Q once IN has been TRUE for PT; IN FALSE resets
   both. *)
FUNCTION_BLOCK TON
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF NOT IN THEN
        Q := FALSE;
        RUNNING := FALSE;
        ET := T#0s;
    ELSE
        IF NOT RUNNING THEN
            RUNNING := TRUE;
            START := CLOCK;
        END_IF;
        ET := CLOCK - START;
        IF ET < T#0s OR ET > PT THEN
            ET := PT;
        END_IF;
        IF ET < T#0s THEN
            ET := T#0s;
        END_IF;
        Q := ET >= PT;
    END_IF;
END_FUNCTION_BLOCK

(* Table 46 feature 2b; Figure 15 b). Q once IN has been TRUE for PT; IN FALSE resets
   both. *)
FUNCTION_BLOCK TON_TIME
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF NOT IN THEN
        Q := FALSE;
        RUNNING := FALSE;
        ET := T#0s;
    ELSE
        IF NOT RUNNING THEN
            RUNNING := TRUE;
            START := CLOCK;
        END_IF;
        ET := CLOCK - START;
        IF ET < T#0s OR ET > PT THEN
            ET := PT;
        END_IF;
        IF ET < T#0s THEN
            ET := T#0s;
        END_IF;
        Q := ET >= PT;
    END_IF;
END_FUNCTION_BLOCK

(* Table 46 feature 3a; Figure 15 c). Q with IN, and for PT after IN falls; ET counts that
   PT, holds it once Q is FALSE, and is 0 while IN is TRUE. *)
FUNCTION_BLOCK TOF
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
    IN_M : BOOL;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF IN THEN
        Q := TRUE;
        RUNNING := FALSE;
        ET := T#0s;
    ELSE
        IF IN_M THEN
            RUNNING := TRUE;
            START := CLOCK;
        END_IF;
        IF RUNNING THEN
            ET := CLOCK - START;
            IF ET < T#0s OR ET > PT THEN
                ET := PT;
            END_IF;
            IF ET < T#0s THEN
                ET := T#0s;
            END_IF;
            IF ET >= PT THEN
                Q := FALSE;
                RUNNING := FALSE;
            END_IF;
        END_IF;
    END_IF;
    IN_M := IN;
END_FUNCTION_BLOCK

(* Table 46 feature 3b; Figure 15 c). Q with IN, and for PT after IN falls; ET counts that
   PT, holds it once Q is FALSE, and is 0 while IN is TRUE. *)
FUNCTION_BLOCK TOF_TIME
VAR_INPUT
    IN : BOOL;
    PT : TIME;
END_VAR
VAR_OUTPUT
    Q : BOOL;
    ET : TIME;
END_VAR
VAR
    RUNNING : BOOL;
    START : TIME;
    CLOCK : TIME;
    IN_M : BOOL;
END_VAR
    IF __NOW() < CLOCK THEN
        START := __NOW() - ET;
    END_IF;
    CLOCK := __NOW();
    IF IN THEN
        Q := TRUE;
        RUNNING := FALSE;
        ET := T#0s;
    ELSE
        IF IN_M THEN
            RUNNING := TRUE;
            START := CLOCK;
        END_IF;
        IF RUNNING THEN
            ET := CLOCK - START;
            IF ET < T#0s OR ET > PT THEN
                ET := PT;
            END_IF;
            IF ET < T#0s THEN
                ET := T#0s;
            END_IF;
            IF ET >= PT THEN
                Q := FALSE;
                RUNNING := FALSE;
            END_IF;
        END_IF;
    END_IF;
    IN_M := IN;
END_FUNCTION_BLOCK

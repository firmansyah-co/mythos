TYPE
  Limits : STRUCT lo : INT := -10; hi : INT := 10; END_STRUCT;
  Channel : STRUCT raw : INT; range : Limits; gain : REAL := 1.5; ok : BOOL; END_STRUCT;
END_TYPE

FUNCTION InRange : BOOL
VAR_INPUT v : INT; r : Limits; END_VAR
  InRange := (v >= r.lo) AND (v <= r.hi);
END_FUNCTION

FUNCTION Total : DINT
VAR_INPUT v : ARRAY [1..5] OF INT; END_VAR
VAR i : INT; END_VAR
  FOR i := 1 TO 5 DO Total := Total + v[i]; END_FOR;
END_FUNCTION

FUNCTION_BLOCK Average
VAR_INPUT sample : INT; END_VAR
VAR_OUTPUT mean : REAL; END_VAR
VAR window : ARRAY [0..3] OF INT; pos : INT; i : INT; sum : DINT; END_VAR
  window[pos] := sample;
  pos := (pos + 1) MOD 4;
  sum := 0;
  FOR i := 0 TO 3 DO sum := sum + window[i]; END_FOR;
  mean := DINT_TO_REAL(sum) / 4.0;
END_FUNCTION_BLOCK

PROGRAM Main
VAR
  cnt : INT;
  ch : ARRAY [1..3] OF Channel;
  spare : Channel;
  squares : ARRAY [1..5] OF INT := [1, 4, 9, 16, 25];
  copy : ARRAY [1..5] OF INT;
  around : ARRAY [-2..2] OF DINT;
  grid : ARRAY [0..2, 1..4] OF INT;
  flt : ARRAY [1..2] OF Average;
  timers : ARRAY [1..2] OF TON;
  i, j, k : INT;
  sum_sq, sum_copy : DINT;
  ok1, ok2, ok3, q1, q2 : BOOL;
  raw2, hi3, cell, diag : INT;
  scaled : REAL;
  mid, edge : DINT;
  mean1, mean2 : REAL;
  et2 : TIME;
END_VAR
  cnt := cnt + 1;
  k := (cnt MOD 3) + 1;
  ch[k].raw := cnt * 3 - 40;
  ch[k].range.hi := cnt;
  ch[k].ok := InRange(ch[k].raw, ch[k].range);
  spare := ch[k];
  spare.gain := spare.gain + 0.5;
  IF cnt MOD 7 = 0 THEN ch[1] := spare; END_IF;
  ok1 := ch[1].ok; ok2 := ch[2].ok; ok3 := ch[3].ok;
  raw2 := ch[2].raw;
  hi3 := ch[3].range.hi;
  scaled := INT_TO_REAL(ch[1].raw) * ch[1].gain;

  squares[k] := squares[k] + cnt;
  IF cnt MOD 5 = 0 THEN copy := squares; END_IF;
  sum_sq := Total(squares);
  sum_copy := Total(copy);

  FOR i := -2 TO 2 DO around[i] := around[i] + i * cnt; END_FOR;
  mid := around[0];
  edge := around[k - 3] - around[2];

  FOR i := 0 TO 2 DO
    FOR j := 1 TO 4 DO
      grid[i, j] := i * 10 + j + cnt;
    END_FOR;
  END_FOR;
  cell := grid[k - 1, k + 1];
  diag := grid[0, 1] + grid[1, 2] + grid[2, 3];

  flt[1](sample := cnt * 2, mean => mean1);
  flt[k MOD 2 + 1](sample := -cnt);
  mean2 := flt[2].mean;

  timers[1](IN := (cnt MOD 10) < 6, PT := T#60ms, Q => q1);
  timers[2](IN := cnt > 5, PT := T#200ms);
  q2 := timers[2].Q;
  et2 := timers[2].ET;
END_PROGRAM

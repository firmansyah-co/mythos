PROGRAM Main
VAR_INPUT
  start_pb, stop_pb, level_hi, level_lo : BOOL;
  pv : REAL;
END_VAR
VAR_OUTPUT
  pump, valve, alarm : BOOL;
  cv : REAL;
END_VAR
VAR
  step : INT;
  t_fill, t_drain, t_alarm, t1, t2, t3, t4, t5 : TON;
  batches : CTU;
  edge : R_TRIG;
  sp : REAL := 50.0;
  kp : REAL := 1.2;
  ki : REAL := 0.05;
  err, integral : REAL;
  i : INT;
  total : DINT;
END_VAR
  CASE step OF
    0: IF start_pb AND NOT stop_pb THEN step := 1; END_IF;
    1: pump := TRUE; valve := FALSE;
       t_fill(IN := TRUE, PT := T#30s);
       IF level_hi OR t_fill.Q THEN step := 2; t_fill(IN := FALSE); END_IF;
    2: pump := FALSE; valve := TRUE;
       t_drain(IN := TRUE, PT := T#20s);
       IF level_lo OR t_drain.Q THEN step := 0; valve := FALSE; t_drain(IN := FALSE); END_IF;
  ELSE
    step := 0;
  END_CASE;
  IF stop_pb THEN step := 0; pump := FALSE; valve := FALSE; END_IF;

  edge(CLK := step = 2);
  batches(CU := edge.Q, R := FALSE, PV := 1000);

  err := sp - pv;
  integral := integral + err * ki;
  IF integral > 100.0 THEN integral := 100.0; ELSIF integral < 0.0 THEN integral := 0.0; END_IF;
  cv := kp * err + integral;
  IF cv > 100.0 THEN cv := 100.0; ELSIF cv < 0.0 THEN cv := 0.0; END_IF;

  t_alarm(IN := pv > 90.0, PT := T#5s);
  alarm := t_alarm.Q;
  t1(IN := pump, PT := T#1s); t2(IN := valve, PT := T#2s); t3(IN := alarm, PT := T#3s);
  t4(IN := t1.Q, PT := T#4s); t5(IN := t2.Q, PT := T#5s);

  total := 0;
  FOR i := 1 TO 50 DO
    total := total + i * 2;
  END_FOR;
END_PROGRAM

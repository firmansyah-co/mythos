FUNCTION Clamp : INT
VAR_INPUT x : INT; lo : INT; hi : INT := 100; END_VAR
  IF x > hi THEN Clamp := hi; ELSIF x < lo THEN Clamp := lo; ELSE Clamp := x; END_IF;
END_FUNCTION

FUNCTION Blend : REAL
VAR_INPUT a, b : REAL; weight : REAL := 0.5; END_VAR
VAR t : REAL := 1.0; END_VAR
  t := t - weight;
  Blend := a * t + b * weight;
END_FUNCTION

FUNCTION Steps : DINT
VAR_INPUT n : INT; END_VAR
VAR i : INT; acc : DINT := 3; END_VAR
  FOR i := 1 TO n DO
    acc := acc + Clamp(i * 7, 0, 20);
  END_FOR;
  Steps := acc;
END_FUNCTION

FUNCTION_BLOCK Climber
VAR_INPUT up : BOOL; rate : INT := 2; END_VAR
VAR_OUTPUT value : INT; top : BOOL; END_VAR
VAR calls : DINT; END_VAR
  calls := calls + 1;
  IF up THEN value := value + rate; ELSE value := value - 1; END_IF;
  value := Clamp(value, -5, 50);
  top := value >= 50;
END_FUNCTION_BLOCK

FUNCTION_BLOCK Stage
VAR_INPUT run : BOOL; END_VAR
VAR_OUTPUT level : INT; done : BOOL; laps : INT; END_VAR
VAR rmp : Climber; edge : R_TRIG; count : CTU; hold : TON; END_VAR
  rmp(up := run, value => level);
  edge(CLK := rmp.top);
  count(CU := edge.Q, PV := 100);
  laps := count.CV;
  hold(IN := rmp.top, PT := T#60ms);
  done := hold.Q;
END_FUNCTION_BLOCK

PROGRAM Main
VAR
  cnt : INT;
  s1, s2 : Stage;
  r1 : Climber;
  c1, c2, c3 : INT;
  m1, m2 : REAL;
  st : DINT;
  l1, l2, laps1, laps2, direct : INT;
  d1, d2, t1, t2 : BOOL;
  nested : INT;
END_VAR
  cnt := cnt + 1;
  c1 := Clamp(cnt * 9, 10, 200);
  c2 := Clamp(x := cnt * 9, lo := 10);
  c3 := Clamp(hi := 40, x := cnt - 30, lo := -8);
  m1 := Blend(INT_TO_REAL(cnt), 10.0, 0.25);
  m2 := Blend(a := 4.0, b := INT_TO_REAL(c1));
  st := Steps(cnt MOD 9);
  nested := Clamp(Clamp(cnt, 5, 30), Clamp(cnt, 0, 12), Clamp(cnt * 2, 20, 45));
  s1(run := (cnt MOD 40) < 30, level => l1, done => d1, laps => laps1);
  s2(run := cnt > 20);
  l2 := s2.level;
  d2 := s2.done;
  laps2 := s2.laps;
  t1 := s1.rmp.top;
  t2 := s2.rmp.top;
  r1.rate := 5;
  r1(up := cnt MOD 3 <> 0);
  direct := r1.value;
END_PROGRAM

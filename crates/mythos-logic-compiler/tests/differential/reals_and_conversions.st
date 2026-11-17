PROGRAM Main
VAR
  cnt : INT;
  x : REAL;
  y : LREAL;
  half : REAL;
  r1, r2, r3 : INT;
  d1, d2 : DINT;
  l1 : LINT;
  avg : REAL;
  sum : REAL;
  big : LREAL := 123456789.75;
  ok : BOOL;
  b2i : INT;
END_VAR
  cnt := cnt + 1;
  x := INT_TO_REAL(cnt) * 0.5 - 3.0;
  half := x / 2.0;
  y := REAL_TO_LREAL(x) * 1.5;
  r1 := REAL_TO_INT(x);
  r2 := REAL_TO_INT(half);
  r3 := LREAL_TO_INT(y);
  d1 := LREAL_TO_DINT(big);
  d2 := REAL_TO_DINT(-x * 100.25);
  l1 := DINT_TO_LINT(d1) * 1000;
  sum := sum + x;
  avg := sum / INT_TO_REAL(cnt);
  ok := (x > 0.0) XOR (cnt MOD 3 = 0) OR NOT (cnt < 5) AND TRUE;
  b2i := BOOL_TO_INT(ok) + 10;
END_PROGRAM

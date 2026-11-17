FUNCTION Hyp : LREAL
VAR_INPUT x, y : LREAL; END_VAR
  Hyp := SQRT(x * x + y * y);
END_FUNCTION

PROGRAM Main
VAR
  n : INT;
  angle : LREAL;
  s, c, t : LREAL;
  rs : REAL;
  root : LREAL;
  lg, ln10, ex : LREAL;
  as1, ac1, at1, at2 : LREAL;
  pw, pwi, pwr : LREAL;
  mag : LREAL;
  big, small, clamped : INT;
  pick, chosen, hundred, twice : INT;
  flag : BOOL;
  absv : INT;
  absr : LREAL;
  sum, prod : INT;
  ordered : BOOL;
  longest, grown : TIME;
END_VAR
  n := n + 1;
  angle := INT_TO_LREAL(n) * 0.37 - 7.0;
  s := SIN(angle);
  c := COS(angle);
  t := TAN(angle * 0.1);
  rs := SIN(LREAL_TO_REAL(angle));
  root := SQRT(INT_TO_LREAL(n) * 3.5);
  lg := LOG(INT_TO_LREAL(n) * 12.5);
  ln10 := LN(INT_TO_LREAL(n) + 0.25);
  ex := EXP(angle * 0.5);
  as1 := ASIN(s * 0.9);
  ac1 := ACOS(c * 0.9);
  at1 := ATAN(angle);
  at2 := ATAN2(s, c);
  pw := EXPT(1.5, angle);
  pwi := EXPT(1.25, n);
  pwr := (INT_TO_LREAL(n) * 0.5) ** 1.5;
  mag := Hyp(s * 3.0, c * 4.0);
  big := MAX(n, 10, 30 - n);
  small := MIN(n, 10, 30 - n);
  clamped := LIMIT(5, n * 2 - 20, 40);
  pick := n MOD 3;
  hundred := n * 100;
  twice := n * 2;
  chosen := MUX(pick, hundred, twice, n);
  flag := SEL(n > 30, FALSE, TRUE);
  absv := ABS(25 - n);
  absr := ABS(angle);
  sum := ADD(n, 1, 2);
  prod := MUL(n, 2);
  ordered := LT(5, n, 50);
  grown := T#25ms * n;
  longest := MAX(T#1s, grown);
END_PROGRAM

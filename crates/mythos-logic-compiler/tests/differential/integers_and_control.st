PROGRAM Main
VAR
  i : INT := 32767; s : SINT := -128; d : DINT := 2147483647;
  a, b, c, n : INT; x : REAL := 2.5; k : DINT := 70000;
  sel : INT; which : INT; sum : DINT; j : INT; m : INT;
  q : DINT; r : DINT; neg : INT := -7;
END_VAR
  i := i + 1;
  s := s - 1;
  d := d + 1;
  a := REAL_TO_INT(x);
  b := REAL_TO_INT(-x);
  c := REAL_TO_INT(3.5);
  n := DINT_TO_INT(k);
  sel := sel + 1;
  CASE sel OF
    1, 2: which := 10;
    3..5: which := 20;
  ELSE which := 99;
  END_CASE;
  sum := 0;
  FOR j := 1 TO 10 DO
    IF j = 7 THEN EXIT; END_IF;
    sum := sum + j;
  END_FOR;
  m := neg / 2;
  q := neg MOD 3;
  r := INT_TO_DINT(neg) * 1000;
END_PROGRAM

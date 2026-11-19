PROGRAM Main
VAR
  cnt : INT;
  in1, in2 : BOOL;
  on_delay, off_delay, pulse : TON;
  tof1 : TOF;
  tp1 : TP;
  ctu1 : CTU;
  ctd1 : CTD;
  rise : R_TRIG;
  fall : F_TRIG;
  latch : SR;
  reset_wins : RS;
  q_on, q_off, q_tp, q_rise, q_fall, q_sr, q_rs : BOOL;
  et_on, et_off, et_tp : TIME;
  up, down : INT;
END_VAR
  cnt := cnt + 1;
  in1 := (cnt MOD 10) < 5;
  in2 := (cnt MOD 7) = 0;
  on_delay(IN := in1, PT := T#60ms, Q => q_on, ET => et_on);
  tof1(IN := in1, PT := T#40ms, Q => q_off, ET => et_off);
  tp1(IN := in2, PT := T#50ms, Q => q_tp, ET => et_tp);
  rise(CLK := in1, Q => q_rise);
  fall(CLK := in1, Q => q_fall);
  ctu1(CU := in1, R := cnt = 40, PV := 3, CV => up);
  ctd1(CD := in1, LD := cnt = 1, PV := 5, CV => down);
  latch(S1 := in2, R := in1, Q1 => q_sr);
  reset_wins(S := in2, R1 := in1, Q1 => q_rs);
END_PROGRAM

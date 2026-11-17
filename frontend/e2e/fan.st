(* The program scripts/e2e.sh loads into a runtime: while the simulated device's seconds
   counter is read, it commands the device's writable register to its setpoint. *)
PROGRAM Fan
VAR_INPUT seconds : INT; seconds_ok : BOOL; setpoint : INT := 300; END_VAR
VAR_OUTPUT speed : INT; END_VAR
  IF seconds_ok THEN
    speed := setpoint;
  ELSE
    speed := 0;
  END_IF;
END_PROGRAM

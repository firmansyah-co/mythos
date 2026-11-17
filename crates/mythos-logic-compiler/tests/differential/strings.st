FUNCTION Tag : STRING
VAR_INPUT prefix : STRING(8); n : INT; END_VAR
  Tag := CONCAT(CONCAT(prefix, '-'), INT_TO_STRING(n));
END_FUNCTION

FUNCTION_BLOCK Shifter
VAR_INPUT text : STRING(12); END_VAR
VAR_OUTPUT rolled : STRING(12); turns : INT; END_VAR
  IF LEN(text) > 1 THEN
    rolled := CONCAT(RIGHT(text, LEN(text) - 1), LEFT(text, 1));
  ELSE
    rolled := text;
  END_IF;
  turns := turns + 1;
END_FUNCTION_BLOCK

PROGRAM Main
VAR
  n : INT;
  digits : STRING(20);
  msg : STRING(40);
  short : STRING(4);
  head : STRING(10);
  tail : STRING(10);
  middle : STRING(10);
  cut : STRING(40);
  swapped : STRING(40);
  grown : STRING(40);
  letters : STRING(12) := 'ABCBCD';
  quoted : STRING(12) := 'it$'s $$5$41';
  size : INT;
  found : INT;
  back : DINT;
  hex : DINT;
  hexText : STRING(8) := '16#7F';
  five : STRING(2) := '5';
  less : BOOL;
  same : BOOL;
  label : STRING(16);
  ring : Shifter;
  rolled : STRING(12);
  names : ARRAY [1..3] OF STRING(6) := ['one', 'two', 'three'];
  pick : STRING(6);
END_VAR
  n := n + 1;
  digits := INT_TO_STRING(n * 37 - 100);
  msg := CONCAT(CONCAT('n=', digits), CONCAT(';', letters));
  short := msg;
  size := LEN(msg);
  head := LEFT(msg, 3);
  tail := RIGHT(digits, 1);
  middle := MID(msg, 2, 2);
  found := FIND(msg, 'BC');
  cut := DELETE(msg, 2, 1);
  swapped := REPLACE(msg, 'XY', 1, 3);
  grown := INSERT(letters, digits, 2);
  back := STRING_TO_DINT(digits);
  hex := STRING_TO_DINT(hexText);
  less := digits < five;
  same := short = 'n=-6';
  label := Tag('pump', n MOD 7);
  ring(text := letters);
  rolled := ring.rolled;
  letters := rolled;
  pick := names[(n MOD 3) + 1];
END_PROGRAM

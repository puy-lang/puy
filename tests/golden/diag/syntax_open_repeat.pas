program SyntaxOpenRepeat;
var X: Integer;
begin
  X := 3;
  repeat
    X := X - 1;
    begin
      WriteLn(X);

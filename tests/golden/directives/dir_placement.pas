program DirPlacement;
var
  X: Integer;
procedure P(A: Integer {$IFDEF PUY}; B: Integer {$ENDIF});
begin
end;
begin
  X := 1 {$IFDEF PUY} + 1 {$ELSE} + 2 {$ENDIF};
  X := 1 {$R-} + 1;
  if {$IFDEF PUY} X {$ENDIF} > 0 then {$Q-} X := 2;
  {$R+}
  X := X {$WARN unused-variable OFF} + 1;
  P(X, {$IFDEF PUY} X {$ENDIF});
  WriteLn(X);
end.

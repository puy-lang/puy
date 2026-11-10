program DirCaseBoundaries;

// Directives between the arms of a `case` statement (after `of`, after the `;` of an arm, before
// `else`, before `end`) and between the statements of its `else` part are at a boundary
// (directives.md 3.1); inside the selector they are not (W0005).

var
  I: Integer;
begin
  for I := 1 to 4 do
    case I of
    {$IFDEF NEVER}
      1: WriteLn('never');
    {$ENDIF}
      1: WriteLn('one');
    {$IFDEF PUY}
      2: WriteLn('two');
    {$ENDIF}
    {$IFDEF NEVER}
    else
      WriteLn('never');
    {$ENDIF}
    {$IFDEF PUY}
    else
      WriteLn('else');
      {$R+}
      WriteLn('else again');
    {$ENDIF}
    end;
  I := 5;
  case I {$IFDEF NEVER} + 1 {$ENDIF} of
    5: WriteLn('five');
  end;
end.

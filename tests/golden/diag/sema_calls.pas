program SemaCalls;
var
  I: Integer;
  S: string;
procedure P(A: Integer);
begin
  WriteLn(A);
end;
function F: Integer;
begin
  Result := 1;
end;
begin
  I := 0;
  S := 'abc';
  P(1, 2);
  P();
  P;
  F(1);
  I := Length(S, 1);
  I := Length();
  I := Copy(S);
  I := Pos(S);
  Inc(I, 1, 2);
  I := S[1, 2];
  I := I[1];
  WriteLn(S['a']);
  WriteLn(S[True]);
  WriteLn(I:3:2);
  WriteLn(P);
  P(F:3);
  I := P;
  WriteLn(Integer);
  I := Integer;
  I := string;
  I := System.SysUtils;
  Result := 1;
  Break;
  Continue;
  Exit(1);
  var N;
  var M: Integer;
  var Q := I;
  WriteLn(M, Q);
end.

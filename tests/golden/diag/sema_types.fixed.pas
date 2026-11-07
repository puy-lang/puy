program SemaTypes;
var
  I: Integer;
  S: string;
  B: Boolean;
  C: Char;
  U: UInt64;
  By: Byte;
procedure V(var A: Integer);
begin
  A := 1;
end;
procedure O(out A: string);
begin
  A := '';
end;
begin
  I := 'abc';
  S := 1;
  B := 1;
  I := True;
  C := 'ab';
  By := 1;
  V(By);
  V(S);
  O(C);
  I := S + 1;
  S := S * 2;
  B := not I;
  B := -B;
  I := I and B;
  U := 1;
  I := U + I;
  I := U div 2;
  B := U < I;
  U := U + 1;
  I := Integer(S);
  S := string(I);
  B := Boolean(I);
  C := Char(65);
  I := Integer('a');
  I := Integer(1.5);
  WriteLn(I, S, B, C, U, By);
end.

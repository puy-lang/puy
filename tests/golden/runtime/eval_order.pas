program EvalOrder;

var
  S: string;
  Zero: Integer;

function Side(N: Integer): Integer;
begin
  Write('[', N, ']');
  Result := N;
end;

function Idx: Integer;
begin
  Write('<idx>');
  Result := 2;
end;

function Val: Char;
begin
  Write('<val>');
  Result := 'Q';
end;

function Replace: Integer;
begin
  S := 'xyz';
  Result := 1;
end;

begin
  Zero := 0;
  WriteLn(Side(1) + Side(2) * Side(3));
  S := 'abc';
  S[Idx] := Val;
  WriteLn(' ', S);
  S := 'abc';
  S[2] := Chr(Ord('0') + Replace);
  WriteLn(S);
  if (Zero <> 0) and (Side(9) > 0) then WriteLn('no');
  if (Zero = 0) or (Side(9) > 0) then WriteLn('short');
  Write('before');
  WriteLn(' ', Side(1), 1 div Zero);
end.

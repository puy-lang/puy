program NestedAssignOuter;

// An assignment to a variable that a nested routine uses: the right-hand side calls the nested
// routine after its first operand is computed, and the nested routine sees the variable's old
// value (reading it) or its write is replaced by the assignment (writing it).

uses System.SysUtils;

procedure Reads;
var
  Z, X: Integer;
  S: string;

  function Peek: Integer;
  begin
    Result := Z;
  end;

  function PeekS: string;
  begin
    Result := S;
  end;

  function Inner: Integer;
  begin
    Result := Peek * 10;
  end;

begin
  Z := 7;
  X := 1;
  Z := (X + 1) + Peek;
  WriteLn('read: ', Z);
  Z := 7;
  Z := (X + 1) * 3 + Inner;
  WriteLn('indirect: ', Z);
  S := 'old';
  S := IntToStr(X) + '-' + PeekS;
  WriteLn('string: ', S);
  S := 'old';
  S := Copy('abc', 1, 2) + PeekS + PeekS;
  WriteLn('strings: ', S);
end;

procedure Writes;
var
  Z, X: Integer;

  function Bump: Integer;
  begin
    Z := 100;
    Result := 0;
  end;

begin
  Z := 7;
  X := 1;
  Z := (X + 1) + Bump;
  WriteLn('write: ', Z);
end;

begin
  Reads;
  Writes;
end.

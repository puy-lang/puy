program CharImplicit;

// A `Char` passed to a record's `Implicit(const A: string)` (objects.md 11.4): a one-character
// literal, a typed constant, a variable and a subrange variable go through the built-in `Char` to
// `string` conversion first, in a conversion by the type's name, an argument, an assignment, an
// initial value, a result, `Exit`, an array element and an operator's operand. A
// type that also declares `Implicit(const A: Char)` takes that one for any `Char` value.

type
  TStr = record
    S: string;
    class operator Implicit(const A: string): TStr;
    class operator Equal(const A, B: TStr): Boolean;
    function GetHashCode: Int64;
    class operator NotEqual(const A, B: TStr): Boolean;
  end;

  TBoth = record
    S: string;
    class operator Implicit(const A: string): TBoth;
    class operator Implicit(const A: Char): TBoth;
  end;

  TLower = 'a'..'z';

const
  CC: Char = 'z';

class operator TStr.Implicit(const A: string): TStr;
begin
  Result := Default(TStr);
  Result.S := 'str ' + A;
end;

class operator TStr.Equal(const A, B: TStr): Boolean;
begin
  Result := A.S = B.S;
end;

function TStr.GetHashCode: Int64;
begin
  Result := Length(S);
end;

class operator TStr.NotEqual(const A, B: TStr): Boolean;
begin
  Result := A.S <> B.S;
end;

class operator TBoth.Implicit(const A: string): TBoth;
begin
  Result := Default(TBoth);
  Result.S := 'str ' + A;
end;

class operator TBoth.Implicit(const A: Char): TBoth;
begin
  Result := Default(TBoth);
  Result.S := 'char ' + A;
end;

procedure Take(const R: TStr);
begin
  WriteLn('take ', R.S);
end;

function MakeResult: TStr;
begin
  Result := 'r';
end;

function MakeExit: TStr;
begin
  Exit('e');
end;

var
  R: TStr;
  B: TBoth;
  C: Char;
  L: TLower;
  A: array of TStr;

begin
  R := TStr('x');
  WriteLn(R.S);
  Take('x');
  R := 'a';
  WriteLn(R.S);
  WriteLn(MakeResult.S);
  WriteLn(MakeExit.S);
  R := CC;
  WriteLn(R.S);
  C := 'y';
  R := C;
  WriteLn(R.S);
  Take(C);
  L := 'q';
  R := L;
  WriteLn(R.S);
  A := nil;
  SetLength(A, 2);
  A[0] := 'm';
  A[1] := C;
  WriteLn(A[0].S, ' ', A[1].S);
  R := 'k';
  WriteLn(R = 'k', ' ', R <> 'k', ' ', R = 'kk');
  B := 'b';
  WriteLn(B.S);
  B := 'bb';
  WriteLn(B.S);
  B := C;
  WriteLn(B.S);
  B := L;
  WriteLn(B.S);
end.

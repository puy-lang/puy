program SliceEvalOrder;

// A slice evaluates its source, then L, then H (types.md 8.7). A bound that changes the
// source variable does not change the value being sliced: the source is read first.

var
  D: TArray<Integer>;

function Src: string;
begin
  Write('s');
  Result := 'abcdef';
end;

function Lo: Integer;
begin
  Write('l');
  Result := 2;
end;

function Hi: Integer;
begin
  Write('h');
  Result := 3;
end;

function Shrink: Integer;
begin
  SetLength(D, 1);
  Result := 3;
end;

var
  S: string;
  R: TArray<Integer>;
  I: Integer;

function Replace: Integer;
begin
  S := 'xy';
  Result := 4;
end;

begin
  WriteLn(' ', Src[Lo..Hi]);
  D := [1, 2, 3, 4, 5];
  R := D[0..Shrink];
  for I in R do
    Write(I, ' ');
  WriteLn('(', Length(D), ')');
  S := 'abcdef';
  WriteLn(S[2..Replace], ' ', S);
end.

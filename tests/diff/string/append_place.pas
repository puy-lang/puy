program AppendPlace;

// Appending to a string place (S := S + X; types.md 9.1) gives the same values as a new
// concatenation: a copy taken before the append keeps its value, whatever the place is (a local,
// a global, a var parameter, a field of a record, an element of a static or dynamic array).
// Writing a character of a string and growing an array one element at a time likewise leave
// earlier copies alone.

uses System.SysUtils;

type
  TRec = record
    S: string;
    N: Integer;
  end;

var
  G, Saved: string;
  R, R2: TRec;
  Fixed: array[1..2] of string;
  Dyn, Dyn2: array of string;
  Nums, Nums2: array of Integer;
  I: Integer;

procedure AddTo(var V: string; const X: string);
begin
  V := V + X;
end;

procedure Local;
var
  S, T: string;
  C: Char;
  J: Integer;
begin
  S := 'ab';
  T := S;
  S := S + 'c';
  WriteLn(S, ' ', T);
  S := S + S;
  WriteLn(S);
  C := '!';
  S := S + C + 'd' + 'é';
  WriteLn(S, ' ', Length(S));
  T := S;
  for J := 1 to 3 do
    S := S + IntToStr(J);
  WriteLn(S, ' ', T);
end;

begin
  Local;
  G := 'x';
  Saved := G;
  for I := 1 to 5 do
    G := G + 'y';
  WriteLn(G, ' ', Saved);
  Saved := G;
  AddTo(G, 'z');
  AddTo(G, G);
  WriteLn(G, ' ', Saved);
  G[1] := 'X';
  WriteLn(G, ' ', Saved);

  R.S := 'r';
  R.N := 1;
  R2 := R;
  R.S := R.S + 's';
  R.S[1] := 'R';
  WriteLn(R.S, ' ', R2.S);

  Fixed[1] := 'f';
  Fixed[2] := Fixed[1];
  Fixed[1] := Fixed[1] + 'g';
  WriteLn(Fixed[1], ' ', Fixed[2]);

  SetLength(Dyn, 2);
  Dyn[0] := 'd';
  Dyn2 := Dyn;
  Saved := Dyn[0];
  Dyn[0] := Dyn[0] + 'e';
  Dyn[0][1] := 'D';
  WriteLn(Dyn[0], ' ', Dyn2[0], ' ', Saved);

  SetLength(Nums, 0);
  for I := 1 to 10 do
  begin
    SetLength(Nums, Length(Nums) + 1);
    Nums[High(Nums)] := I;
  end;
  Nums2 := Copy(Nums);
  SetLength(Nums, 3);
  SetLength(Nums, 5);
  WriteLn(Length(Nums), ' ', Nums[2], ' ', Nums[4], ' ', Length(Nums2), ' ', Nums2[9]);
end.

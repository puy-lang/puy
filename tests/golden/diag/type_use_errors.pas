program TypeUseErrors;

// Errors in uses of records and static arrays (types.md 2.1, 2.2, chapter 3, chapter 4).

type
  TPoint = record
    X, Y: Integer;
  end;
  TOther = record
    X, Y: Integer;
  end;
  TArr = array[1..3] of Integer;
  TCounts = array['a'..'c'] of Integer;

const
  Origin: TPoint = (X: 0; Y: 0);
  Short: TArr = (1, 2);                    // E0702: element count
  Swapped: TPoint = (Y: 1; X: 2);          // E0702: field order
  Partial: TPoint = (X: 1);                // E0702: missing field
  Extra: TPoint = (X: 1; Y: 2; Z: 3);      // E0700: no such field
  WrongShape: TArr = (X: 1);               // E0702: record constant for an array

procedure Show(const P: TPoint);
begin
  P.X := 1;                                // E0707: const parameter
end;

procedure Bump(var N: Integer);
begin
  Inc(N);
end;

var
  P: TPoint;
  O: TOther;
  A: TArr;
  C: TCounts;
  S: string;
begin
  P := Default(TPoint);
  O := Default(TOther);
  A := Default(TArr);
  C := Default(TCounts);
  S := 'abc';
  P := O;                                  // E0702: different record types
  if P = P then WriteLn('eq');             // E0703
  A := A + A;                              // E0703
  WriteLn(P);                              // E0718
  WriteLn(P.Z);                            // E0700: no field
  A[4] := 1;                               // E0714: constant index out of range
  A['x'] := 1;                             // E0719: index type
  C[1] := 1;                               // E0719: Char index
  A[1, 2] := 3;                            // E0705: too many indices
  P[1] := 2;                               // E0720
  Origin.X := 5;                           // E0707: typed constant
  Bump(Origin.X);                          // E0706
  Bump(S[1]);                              // E0706
  WriteLn(Default(P).X);                   // E0708
  WriteLn(High(S));                        // fine: Length(S)
  WriteLn(Low(P));                         // E0702
end.

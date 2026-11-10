program DynArrayErrors;

// Diagnostics of dynamic arrays, open array parameters and array constructors
// (types.md 2.3, 2.4, chapters 4 and 7).

type
  TInts = array of Integer;
  TStrs = array of string;
  TPair = array[0..1] of Integer;

procedure Fill(const A: array of Integer);
begin
  A[0] := 1;                            // E0707: const open array
end;

procedure Resize(A: array of Integer; var B: array of Integer);
begin
  SetLength(A, 2);                      // E0707: open array parameter
  B := nil;                             // E0707: open array parameter
  A[0] := Length(B);
end;

procedure Strs(const A: array of string);
begin
end;

procedure Bad(out A: array of Integer); // E3200: out open array
begin
end;

procedure Args(const A: array of const); // E3200: array of const (M3)
begin
end;

var
  A, B: TInts;
  S: TStrs;
  P: TPair;
  I: Integer;
  Str: string;
begin
  var X := [1, 2];                      // E0729
  WriteLn([1]);                         // E0729
  I := nil;                             // E0702: nil to Integer
  S := A;                               // E0702: element types differ
  Strs(A);                              // E0702: element types differ
  P := [1, 2, 3];                       // E0702: element count
  A := [1..3];                          // E0702: range in an array constructor
  Str := [1];                           // E0702: not an array type
  if A = B then;                        // E0703: arrays are not compared
  WriteLn(A);                           // E0718
  I := A['x'];                          // E0719
  SetLength(A);                         // E0705
  SetLength(A, 1, 2);                   // E0705: one dimension
  SetLength(I, 1);                      // E0702: not a string or a dynamic array
  SetLength(A, 'x');                    // E0702: length is an integer
  I := Ord(Assigned(I));                // E0702: Assigned of an Integer
  I := High(TInts);                     // E0702: High of a dynamic array type
  I := Copy(A, 1, 2, 3)[0];             // E0705
  if nil = nil then;                    // E3200: nil without a dynamic array
end.

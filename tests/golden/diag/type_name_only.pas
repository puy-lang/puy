program TypeNameOnly;

// Where only a type name can be written (the type of a `for var` loop variable and the argument
// of `TArray<…>`), any other type is E0404 `expected a type name` and is skipped to the end of
// the type, without further errors (grammar.md 1.3).

type
  TRow = array[0..1] of Integer;

var
  A: TArray<array of Integer>;          // E0404
  B: TArray<record X: Integer; end>;    // E0404
  C: TArray<packed record X: Integer; end>; // E0404
  D: TArray<set of Byte>;               // E0404
  E: TArray<(Red, Green)>;              // E0404
  F: TArray<0..9>;                      // E0404
  Rows: TArray<TRow>;
begin
  for var R: array[0..1] of Integer in Rows do       // E0404
    WriteLn(R[0]);
  for var R: packed array[0..1] of Integer in Rows do // E0404
    WriteLn(R[0]);
  for var R: record X: Integer; end in Rows do        // E0404
    WriteLn(R.X); // the loop variable has the error type, not the element type: no E3200
  for var I: 0..9 := 1 to 3 do                         // E0404
    WriteLn(I.X); // likewise, not `Integer`
end.

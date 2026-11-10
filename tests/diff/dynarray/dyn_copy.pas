program DynCopy;

// `Copy` of a dynamic array makes a new dynamic array (types.md chapter 7); the range is cut
// to the array. (`Copy` of open and static arrays is in golden/runtime/copy_any_array.)

type
  TInts = array of Integer;

procedure Show(const Title: string; const A: TInts);
var
  I: Integer;
begin
  Write(Title, ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn(' (', Length(A), ')');
end;

var
  A, B: TInts;
  S: array of string;
begin
  A := [1, 2, 3, 4, 5];
  B := Copy(A);
  B[0] := 100;
  Show('A', A);
  Show('B', B);
  Show('mid', Copy(A, 1, 3));
  Show('from', Copy(A, 3));
  Show('long', Copy(A, 2, 100));
  Show('past', Copy(A, 9, 2));
  Show('none', Copy(A, 1, 0));
  S := ['a', 'b', 'c'];
  S := Copy(S, 1, 2);
  WriteLn(S[0], S[1], ' ', Length(S));
end.

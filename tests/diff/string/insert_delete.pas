program InsertDelete;

// Insert / Delete / Concat of strings (semantics.md 5.3): positions outside the string are cut
// to it (Insert at Index < 1 goes to the start and past the end to the end; Delete outside the
// string or with Count <= 0 does nothing), as in Delphi.

var
  S: string;
  A: array[0..1] of string;
  C: Char;
begin
  S := 'world';
  Insert('hello ', S, 1);
  WriteLn(S);
  Insert('!', S, 100);
  WriteLn(S);
  Insert('>', S, -5);
  WriteLn(S);
  Insert('', S, 3);
  WriteLn(S);
  C := ',';
  Insert(C, S, 7);
  WriteLn(S);
  Delete(S, 1, 1);
  WriteLn(S);
  Delete(S, 6, 1);
  WriteLn(S);
  Delete(S, 7, 1000);
  WriteLn(S);
  Delete(S, 0, 2);
  Delete(S, 100, 2);
  Delete(S, 2, 0);
  Delete(S, 2, -1);
  WriteLn(S);
  S := 'abc';
  Insert(S, S, 2);
  WriteLn(S);
  A[1] := 'xyz';
  Insert('-', A[1], 2);
  Delete(A[1], 1, 1);
  WriteLn(A[1]);
  S := 'あいう';
  Insert('え', S, 2);
  Delete(S, 1, 1);
  WriteLn(S, ' ', Length(S));
  WriteLn(Concat('a'), Concat('b', 'c'), Concat('d', C, 'e', S));
  S := Concat(S, '!');
  WriteLn(S);
end.

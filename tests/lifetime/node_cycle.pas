program NodeCycle;

// A record element that holds its own array forms a cycle, which reference counting does not
// free (requirements §5.2): the leak check reports the array and the record
// (lifetime.md 3.11, 3.12).

type
  TNode = record
    Kids: array of TNode;
    Tag: string;
  end;

var
  A: array of TNode;
begin
  SetLength(A, 1);
  A[0].Kids := A;
  WriteLn(Length(A[0].Kids));
end.

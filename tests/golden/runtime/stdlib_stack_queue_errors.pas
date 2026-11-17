// The exceptions of TStack<T> and TQueue<T> (docs/spec/collections.md 3.2, 3.5, chapter 6):
// taking from an empty stack or queue raises EListError (`stack is empty` / `queue is empty`) for
// Pop, Peek, Extract and Dequeue; writing a capacity below the count raises ERangeError. A member
// that raised leaves the container as it was.
program StdlibStackQueueErrors;

uses System.SysUtils, System.Generics.Collections;

var
  S: TStack<Integer>;
  Q: TQueue<string>;
  X: Integer;

procedure Show(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  S := TStack<Integer>.Create;
  try S.Pop; except on E: Exception do Show(E); end;
  try S.Peek; except on E: Exception do Show(E); end;
  try S.Extract; except on E: Exception do Show(E); end;
  S.Push(1);
  S.Push(2);
  S.Push(3);
  try
    S.Capacity := 2;
  except
    on E: Exception do Show(E);
  end;
  WriteLn(S.Count, ' ', S.Capacity);
  for X in S do
    Write(X, ' ');
  WriteLn;
  S.Pop;
  S.Pop;
  S.Pop;
  try S.Pop; except on E: EListError do Show(E); end;
  WriteLn(S.Count);
  S.Free;

  Q := TQueue<string>.Create;
  try Q.Dequeue; except on E: Exception do Show(E); end;
  try Q.Peek; except on E: Exception do Show(E); end;
  try Q.Extract; except on E: Exception do Show(E); end;
  Q.Enqueue('a');
  Q.Enqueue('b');
  try
    Q.Capacity := -1;
  except
    on E: ERangeError do Show(E);
  end;
  WriteLn(Q.Count, ' ', Q.Peek);
  Q.Dequeue;
  Q.Dequeue;
  try Q.Dequeue; except on E: Exception do Show(E); end;
  Q.Free;
end.

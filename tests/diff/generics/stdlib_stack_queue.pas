// The basic operations of TStack<T> and TQueue<T> (docs/spec/collections.md chapter 6): Push /
// Pop / Peek and Enqueue / Dequeue / Peek, Count, Clear, ToArray and for..in (a stack from the
// bottom, a queue from the first item).
program StdlibStackQueue;

uses System.SysUtils, System.Generics.Collections;

var
  S: TStack<Integer>;
  Q: TQueue<string>;
  Words: TStack<string>;
  A: TArray<Integer>;
  X, I: Integer;
  W: string;
begin
  S := TStack<Integer>.Create;
  for I := 1 to 10 do
    S.Push(I * I);
  WriteLn(S.Count, ' ', S.Peek);
  WriteLn(S.Pop, ' ', S.Pop, ' ', S.Count);
  for X in S do
    Write(X, ' ');
  WriteLn;
  A := S.ToArray;
  WriteLn(Length(A), ' ', A[0], ' ', A[High(A)]);
  while S.Count > 3 do
    S.Pop;
  S.Push(-1);
  for X in S do
    Write(X, ' ');
  WriteLn;
  S.Clear;
  WriteLn(S.Count);
  S.Push(7);
  WriteLn(S.Peek, ' ', S.Count);
  S.Free;

  Q := TQueue<string>.Create;
  Q.Enqueue('alpha');
  Q.Enqueue('beta');
  Q.Enqueue('gamma');
  WriteLn(Q.Count, ' ', Q.Peek);
  WriteLn(Q.Dequeue, ' ', Q.Count);
  // Interleave so that the items wrap round inside the queue
  for I := 1 to 20 do
  begin
    Q.Enqueue('w' + IntToStr(I));
    W := Q.Dequeue;
    if I mod 5 = 0 then
      Write(W, ' ');
  end;
  WriteLn;
  for W in Q do
    Write(W, ' ');
  WriteLn;
  for W in Q.ToArray do
    Write(W, ' ');
  WriteLn;
  while Q.Count > 0 do
    Write(Q.Dequeue, ' ');
  WriteLn;
  WriteLn(Q.Count);
  Q.Free;

  Words := TStack<string>.Create;
  Words.Push('x');
  Words.Push('y');
  W := Words.Peek;
  Words.Push(W + W);
  while Words.Count > 0 do
    Write(Words.Pop, ' ');
  WriteLn;
  Words.Free;
end.

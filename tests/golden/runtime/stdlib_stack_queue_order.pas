// The order of TStack<T> and TQueue<T> (docs/spec/collections.md 3.4, 3.5, chapter 6): a stack
// enumerates and copies (ToArray) from the bottom, a queue from the first item, also after the
// items of the queue have wrapped round its ring and after the capacity was written (grown,
// shrunk, TrimExcess) while they were wrapped. The capacity doubles as items are added. The
// constructors push or enqueue in order, from an array or from another container (TEnumerable<T>).
program StdlibStackQueueOrder;

uses System.SysUtils, System.Generics.Collections;

procedure ShowArray(const Title: string; const A: TArray<Integer>);
var
  X: Integer;
begin
  Write(Title, ':');
  for X in A do
    Write(' ', X);
  WriteLn;
end;

procedure ShowQueue(const Title: string; Q: TQueue<Integer>);
var
  X: Integer;
begin
  Write(Title, ' (', Q.Count, '/', Q.Capacity, '):');
  for X in Q do
    Write(' ', X);
  WriteLn;
  ShowArray('  ToArray', Q.ToArray);
end;

var
  S, S2: TStack<Integer>;
  Q, Q2: TQueue<Integer>;
  I: Integer;
  Caps: string;
begin
  S := TStack<Integer>.Create;
  Caps := '';
  for I := 1 to 9 do
  begin
    S.Push(I);
    Caps := Caps + ' ' + IntToStr(S.Capacity);
  end;
  WriteLn('capacities:', Caps);
  ShowArray('stack ToArray', S.ToArray);
  Write('stack for-in:');
  for I in S do
    Write(' ', I);
  WriteLn;
  WriteLn('Extract ', S.Extract, ', Pop ', S.Pop, ', Peek ', S.Peek);
  S.TrimExcess;
  WriteLn('after TrimExcess: ', S.Count, '/', S.Capacity);
  S.Capacity := 20;
  WriteLn('after Capacity := 20: ', S.Count, '/', S.Capacity);
  ShowArray('stack ToArray', S.ToArray);

  // From an array, and from another container (enumerated bottom first)
  S2 := TStack<Integer>.Create([10, 20, 30]);
  WriteLn('S2 Pop ', S2.Pop, ', Count ', S2.Count, ', Capacity ', S2.Capacity);
  Q2 := TQueue<Integer>.Create(S);
  ShowQueue('queue from stack', Q2);
  Q2.Free;
  S2.Free;
  S.Free;

  // Wrap the items round the ring: capacity 4, first item in slot 2
  Q := TQueue<Integer>.Create;
  for I := 1 to 4 do
    Q.Enqueue(I);
  Q.Dequeue;
  Q.Dequeue;
  Q.Enqueue(5);
  Q.Enqueue(6);
  ShowQueue('wrapped', Q);
  // Growing keeps the order and puts the first item in front
  Q.Enqueue(7);
  ShowQueue('grown', Q);
  Q.Dequeue;
  Q.Dequeue;
  Q.Dequeue;
  for I := 8 to 11 do
    Q.Enqueue(I);
  ShowQueue('wrapped again', Q);
  Q.Capacity := 10;
  ShowQueue('Capacity := 10', Q);
  Q.Dequeue;
  Q.Enqueue(12);
  for I := 1 to 6 do
  begin
    Q.Enqueue(Q.Dequeue);
  end;
  ShowQueue('rotated', Q);
  Q.TrimExcess;
  ShowQueue('TrimExcess', Q);
  Q.Enqueue(13);
  ShowQueue('after TrimExcess', Q);
  Q.Clear;
  ShowQueue('Clear', Q);
  Q.Enqueue(14);
  ShowQueue('after Clear', Q);
  Q.Capacity := 1;
  ShowQueue('Capacity := 1', Q);
  Q.Dequeue;
  Q.TrimExcess;
  ShowQueue('empty TrimExcess', Q);
  Q.Enqueue(15);
  ShowQueue('after empty', Q);
  Q.Free;
end.

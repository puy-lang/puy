// Changing a TStack<T> or TQueue<T> during for..in (docs/spec/collections.md 3.3) raises
// EInvalidOperation at the next MoveNext, also when the change is made in the last iteration.
// Peek, ToArray and writing the capacity it already has are not changes; writing another
// capacity, Clear and TrimExcess that frees slots are. The name in the message is the class name
// of the container, with the type arguments as full names (TStack<StdlibStackQueueModified.TFoo>),
// or the name of a user's class derived from it.
program StdlibStackQueueModified;

uses System.SysUtils, System.Generics.Collections;

type
  TFoo = class
  end;

  TMyQueue = class(TQueue<Integer>)
  end;

procedure Show(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

var
  S: TStack<Integer>;
  Q: TQueue<Integer>;
  M: TMyQueue;
  Foos: TStack<TFoo>;
  F: TFoo;
  X, N: Integer;
begin
  S := TStack<Integer>.Create;
  S.Push(1);
  S.Push(2);
  S.Push(3);
  try
    for X in S do
    begin
      WriteLn('visit ', X);
      S.Push(X);
    end;
  except
    on E: Exception do Show(E);
  end;
  // A change in the last iteration is found by the MoveNext that would end the loop
  N := 0;
  try
    for X in S do
    begin
      Inc(N);
      if N = S.Count then
        S.Pop;
    end;
    WriteLn('not reached');
  except
    on E: Exception do Show(E);
  end;
  // Not changes
  N := 0;
  for X in S do
  begin
    Inc(N, S.Peek + Length(S.ToArray));
    S.Capacity := S.Capacity;
  end;
  WriteLn('sum ', N);
  try
    for X in S do
      S.Capacity := S.Capacity + 1;
  except
    on E: Exception do Show(E);
  end;
  try
    for X in S do
      S.Clear;
  except
    on E: Exception do Show(E);
  end;
  S.Free;

  Q := TQueue<Integer>.Create;
  Q.Enqueue(1);
  Q.Enqueue(2);
  try
    for X in Q do
      Q.Dequeue;
  except
    on E: Exception do Show(E);
  end;
  try
    for X in Q do
      Q.TrimExcess;
  except
    on E: Exception do Show(E);
  end;
  Q.Free;

  M := TMyQueue.Create;
  M.Enqueue(1);
  try
    for X in M do
      M.Enqueue(X);
  except
    on E: Exception do Show(E);
  end;
  M.Free;

  Foos := TStack<TFoo>.Create;
  Foos.Push(nil);
  try
    for F in Foos do
      Foos.Push(F);
  except
    on E: Exception do Show(E);
  end;
  Foos.Free;
end.

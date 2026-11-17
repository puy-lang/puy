program StdlibStackQueueLifetime;

// TStack<T> and TQueue<T> count the managed values they hold (docs/spec/collections.md 3.1): an
// instance of a reference counted class stays alive while a stack or queue holds it and is
// destroyed when the last holder lets go of it: when it is popped or dequeued and the result is
// dropped (at the end of the routine that dropped it, like any other discarded result), at
// Clear, and when the container is freed. Records holding instances and strings are let go of in
// the same way, also after the items of a queue have wrapped round and the capacity was changed.
// Freeing a container in the middle of for..in makes the next MoveNext raise EAccessViolation
// (3.4).

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TEntry = record
    Item: TItem;
    Tag: string;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

function Entry(const Name: string): TEntry;
begin
  Result := Default(TEntry);
  Result.Item := TItem.Create(Name);
  Result.Tag := Name + '!';
end;

// The new values are made in routines of their own, so that no temporary of the caller holds them
procedure PushNew(S: TStack<TItem>; const Name: string);
begin
  S.Push(TItem.Create(Name));
end;

procedure EnqueueNew(Q: TQueue<TEntry>; const Name: string);
begin
  Q.Enqueue(Entry(Name));
end;

procedure KeepDequeue(Q: TQueue<TEntry>);
var
  E: TEntry;
begin
  E := Q.Dequeue;
  WriteLn('kept ', E.Tag);
end;

procedure DropPop(S: TStack<TItem>);
begin
  S.Pop;
end;

procedure DropDequeue(Q: TQueue<TEntry>);
begin
  Q.Dequeue;
end;

procedure Stack;
var
  S: TStack<TItem>;
  Kept: TItem;
begin
  S := TStack<TItem>.Create;
  Kept := TItem.Create('a');
  S.Push(Kept);
  PushNew(S, 'b');
  PushNew(S, 'c');
  WriteLn('a is held twice: ', Kept.RefCount);
  Kept := nil;
  WriteLn('drop a pop');
  DropPop(S);
  WriteLn('pop and keep');
  Kept := S.Pop;
  WriteLn('kept ', Kept.Name, ' ', Kept.RefCount);
  Kept := nil;
  // The result of Peek is a temporary of this routine: it holds `a` until the routine ends
  WriteLn('peek ', S.Peek.Name);
  PushNew(S, 'd');
  WriteLn('clear');
  S.Clear;
  WriteLn('push again');
  PushNew(S, 'e');
  PushNew(S, 'f');
  WriteLn('free the stack');
  S.Free;
  WriteLn('stack done');
end;

procedure Queue;
var
  Q: TQueue<TEntry>;
  I: Integer;
begin
  Q := TQueue<TEntry>.Create;
  for I := 1 to 4 do
    EnqueueNew(Q, 'q' + IntToStr(I));
  WriteLn('drop a dequeue');
  DropDequeue(Q);
  KeepDequeue(Q);
  WriteLn('wrap round, then grow while wrapped');
  EnqueueNew(Q, 'q5');
  EnqueueNew(Q, 'q6');
  EnqueueNew(Q, 'q7');
  WriteLn('capacity ', Q.Capacity);
  DropDequeue(Q);
  Q.TrimExcess;
  WriteLn('free the queue');
  Q.Free;
  WriteLn('queue done');
end;

procedure FreeWhileEnumerating;
var
  Strs: TQueue<string>;
  W: string;
begin
  W := '';
  Strs := TQueue<string>.Create;
  Strs.Enqueue('x');
  Strs.Enqueue('y');
  try
    for W in Strs do
    begin
      WriteLn('visit ', W);
      Strs.Free;
    end;
  except
    on Ex: Exception do
      WriteLn(Ex.ClassName, ': ', Ex.Message);
  end;
end;

begin
  Stack;
  Queue;
  FreeWhileEnumerating;
  WriteLn('end');
end.

program StdlibStackQueueClearReentrant;

// A stack and a queue let go of their items at Clear while they already hold none, as a list
// does: a destructor sees Count 0, and the items it adds stay after Clear, where enumerating,
// Pop and Dequeue return them (collections.md 9.4). Before M4-137 a destructor saw the old
// Count, and the items it added were dropped from the count by Clear, left in the array.

uses System.Generics.Collections;

type
  // Shows the count of the container being cleared when it is destroyed, and adds two items to
  // it (an item made by a destructor, with Id 100 and up, does neither).
  TAdder = class(TRefObject)
    Id: Integer;
    constructor Create(AId: Integer);
    destructor Destroy; override;
  end;

  // Shows each item it is told of at cnRemoved, with the count of the stack then.
  TTellingStack = class(TStack<TAdder>)
  protected
    procedure Notify(const Item: TAdder; Action: TCollectionNotification); override;
  end;

  // The same for a queue.
  TTellingQueue = class(TQueue<TAdder>)
  protected
    procedure Notify(const Item: TAdder; Action: TCollectionNotification); override;
  end;

var
  S: TStack<TAdder>;
  Q: TQueue<TAdder>;
  Next: Integer;

constructor TAdder.Create(AId: Integer);
begin
  inherited Create;
  Id := AId;
end;

destructor TAdder.Destroy;
var
  I: Integer;
begin
  if Id < 100 then
  begin
    if S <> nil then
      WriteLn('  destroying ', Id, ': stack count ', S.Count)
    else
      WriteLn('  destroying ', Id, ': queue count ', Q.Count);
    for I := 1 to 2 do
    begin
      Inc(Next);
      if S <> nil then
        S.Push(TAdder.Create(Next))
      else
        Q.Enqueue(TAdder.Create(Next));
    end;
  end;
  inherited;
end;

procedure TTellingStack.Notify(const Item: TAdder; Action: TCollectionNotification);
begin
  if Action = TCollectionNotification.cnRemoved then
    WriteLn('  removed ', Item.Id, ': stack count ', Count);
end;

procedure TTellingQueue.Notify(const Item: TAdder; Action: TCollectionNotification);
begin
  if Action = TCollectionNotification.cnRemoved then
    WriteLn('  removed ', Item.Id, ': queue count ', Count);
end;

procedure ShowStack(const Title: string);
var
  X: TAdder;
begin
  Write(Title, ': count ', S.Count, ', capacity ', S.Capacity, ', items');
  for X in S do
    Write(' ', X.Id);
  Write(', popped');
  while S.Count > 0 do
    Write(' ', S.Pop.Id);
  WriteLn;
end;

// Pushes the items in a routine of its own, so that no temporary of the caller holds the last
// one when the stack is cleared.
procedure FillStack(Count: Integer);
var
  I: Integer;
begin
  for I := 1 to Count do
    S.Push(TAdder.Create(I));
end;

procedure Stack(const Title: string; Capacity, Count: Integer);
begin
  WriteLn(Title);
  S.Capacity := Capacity;
  FillStack(Count);
  S.Clear;
  ShowStack(Title);
  S.Free;
  S := nil;
end;

procedure Stacks;
begin
  // 3 items in a capacity of 4, and 2 in a capacity of 16
  Next := 100;
  S := TStack<TAdder>.Create;
  Stack('stack of capacity 4', 4, 3);
  Next := 200;
  S := TStack<TAdder>.Create;
  Stack('stack of capacity 16', 16, 2);
  // Notify hears of the items after the stack let go of them all
  Next := 300;
  S := TTellingStack.Create;
  Stack('stack with Notify', 4, 2);
  // Freeing a stack that holds items
  Next := 350;
  WriteLn('stack freed');
  S := TStack<TAdder>.Create;
  S.Capacity := 4;
  FillStack(3);
  S.Free;
  S := nil;
end;

procedure ShowQueue(const Title: string);
var
  X: TAdder;
begin
  Write(Title, ': count ', Q.Count, ', capacity ', Q.Capacity, ', items');
  for X in Q do
    Write(' ', X.Id);
  Write(', dequeued');
  while Q.Count > 0 do
    Write(' ', Q.Dequeue.Id);
  WriteLn;
end;

// Enqueues the items From..UpTo (see FillStack).
procedure FillQueue(From, UpTo: Integer);
var
  I: Integer;
begin
  for I := From to UpTo do
    Q.Enqueue(TAdder.Create(I));
end;

// Dequeues an item and lets go of it without showing it (an Id of 100 and up adds nothing).
procedure DropFirst;
begin
  Q.Dequeue.Id := 1000;
end;

// Makes the queue of Capacity, with its first slot at Head, and enqueues the items 1..Count.
procedure FillQueueAt(Capacity, Head, Count: Integer);
var
  I: Integer;
begin
  Q.Capacity := Capacity;
  FillQueue(1, Head);
  for I := 1 to Head do
    DropFirst;
  FillQueue(1, Count);
end;

procedure Queue(const Title: string; Capacity, Head, Count: Integer);
begin
  WriteLn(Title);
  FillQueueAt(Capacity, Head, Count);
  Q.Clear;
  ShowQueue(Title);
  Q.Free;
  Q := nil;
end;

procedure Queues;
begin
  // The whole array is let go of. A ring that wraps round is let go of first to last all the
  // same, from a copy, and one that does not is let go of slot by slot
  Next := 400;
  Q := TQueue<TAdder>.Create;
  Queue('queue that wraps round', 4, 2, 4);
  Next := 450;
  Q := TQueue<TAdder>.Create;
  Queue('queue that has moved on', 4, 1, 3);
  // The slots are written one at a time, wrapping round to slot 0 in the second
  Next := 500;
  Q := TQueue<TAdder>.Create;
  Queue('queue of capacity 16', 16, 0, 2);
  Next := 550;
  Q := TQueue<TAdder>.Create;
  Queue('queue of capacity 16 that wraps round', 16, 14, 3);
  // Notify hears of the items after the queue let go of them all
  Next := 600;
  Q := TTellingQueue.Create;
  Queue('queue with Notify', 4, 2, 3);
  // Freeing a queue that holds items, wrapping round
  Next := 650;
  WriteLn('queue freed');
  Q := TQueue<TAdder>.Create;
  FillQueueAt(4, 3, 3);
  Q.Free;
  Q := nil;
end;

begin
  Stacks;
  Queues;
end.

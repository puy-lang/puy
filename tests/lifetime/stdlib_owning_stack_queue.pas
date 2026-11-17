program StdlibOwningStackQueue;

// TObjectStack<T> and TObjectQueue<T> (collections.md 9.1): Pop and Dequeue are procedures that
// free the item they take off, Extract hands it over without freeing it, and Clear and freeing
// the container free the items bottom first (stack) or first to last (queue, also after the ring
// has wrapped round). Called through a variable whose static type is TStack<T> / TQueue<T>, the
// functions Pop and Dequeue return the item without freeing it, while Clear and Free still free
// the items; so do Delete and Clear on a TList<T> variable holding a TObjectList<T>. Nothing is
// freed while OwnsObjects is False.

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  inherited Destroy;
end;

var
  S: TObjectStack<TItem>;
  Q: TObjectQueue<TItem>;
  PS: TStack<TItem>;
  PQ: TQueue<TItem>;
  PL: TList<TItem>;
  X: TItem;

begin
  WriteLn('-- stack');
  S := TObjectStack<TItem>.Create([TItem.Create('a'), TItem.Create('b'), TItem.Create('c'),
    TItem.Create('d')]);
  S.Pop;
  X := S.Extract;
  WriteLn('extracted ', X.Name);
  X.Free;
  S.Push(TItem.Create('e'));
  S.Clear;
  S.Push(TItem.Create('f'));
  S.Push(TItem.Create('g'));
  S.Free;

  WriteLn('-- queue');
  Q := TObjectQueue<TItem>.Create;
  Q.Enqueue(TItem.Create('a'));
  Q.Enqueue(TItem.Create('b'));
  Q.Enqueue(TItem.Create('c'));
  Q.Enqueue(TItem.Create('d'));
  Q.Dequeue;
  X := Q.Extract;
  WriteLn('extracted ', X.Name);
  X.Free;
  // Wraps round the ring of capacity 4
  Q.Enqueue(TItem.Create('e'));
  Q.Enqueue(TItem.Create('f'));
  Q.Clear;
  Q.Enqueue(TItem.Create('g'));
  Q.Enqueue(TItem.Create('h'));
  Q.Free;

  WriteLn('-- through the parent types');
  PS := TObjectStack<TItem>.Create;
  PS.Push(TItem.Create('a'));
  PS.Push(TItem.Create('b'));
  PS.Push(TItem.Create('c'));
  X := PS.Pop;
  WriteLn('popped ', X.Name);
  X.Free;
  PS.Clear;
  PS.Push(TItem.Create('d'));
  PS.Free;
  PQ := TObjectQueue<TItem>.Create;
  PQ.Enqueue(TItem.Create('a'));
  PQ.Enqueue(TItem.Create('b'));
  X := PQ.Dequeue;
  WriteLn('dequeued ', X.Name);
  X.Free;
  PQ.Free;
  PL := TObjectList<TItem>.Create;
  PL.Add(TItem.Create('a'));
  PL.Add(TItem.Create('b'));
  PL.Add(TItem.Create('c'));
  PL.Delete(1);
  PL[0] := TItem.Create('a2');
  PL.Clear;
  PL.Add(TItem.Create('d'));
  PL.Free;

  WriteLn('-- not owning');
  X := TItem.Create('x');
  S := TObjectStack<TItem>.Create([X, X], False);
  S.Pop;
  S.Free;
  Q := TObjectQueue<TItem>.Create(False);
  Q.Enqueue(X);
  Q.Dequeue;
  Q.Enqueue(X);
  Q.Free;
  WriteLn('still ', X.Name);
  X.Free;
end.

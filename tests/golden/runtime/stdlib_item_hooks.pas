program StdlibItemHooks;

// A user's subclass of TList<T>, TStack<T>, TQueue<T> and TDictionary<K, V> sees items come and
// go through the protected virtual methods Notify, KeyNotify and ValueNotify (collections.md
// 9.4, 9.5). cnAdding is told before an item goes in, and an exception from it leaves the
// container as it was (AddRange tells about every item first); the items added by a larger Count
// are not told about. cnRemoved is told after an item has left, in index order for Clear, and an
// exception from it does not stop the rest. Extract, ExtractAt, reordering and capacity changes
// tell nothing. L[I] := X tells cnAdding for X, then cnRemoved for the item written over unless
// it is X itself (the same instance). The dictionary tells the key, then the value; writing to a
// key already there tells the value written over and the key passed in as removed; TryAdd of a
// key already there and Add of a duplicate key tell nothing. Only the methods a subclass
// overrides are called, also when its constructor does not call the inherited one.

uses System.SysUtils, System.Generics.Collections;

type
  EHook = class(Exception);

  TLogList = class(TList<Integer>)
  protected
    procedure Notify(const Item: Integer; Action: TCollectionNotification); override;
  end;

  TLogStack = class(TStack<string>)
  protected
    procedure Notify(const Item: string; Action: TCollectionNotification); override;
  end;

  TLogQueue = class(TQueue<string>)
  protected
    procedure Notify(const Item: string; Action: TCollectionNotification); override;
  end;

  TLogDict = class(TDictionary<string, Integer>)
  protected
    procedure KeyNotify(const Key: string; Action: TCollectionNotification); override;
    procedure ValueNotify(const Value: Integer; Action: TCollectionNotification); override;
  end;

  // Only the values are told
  TValueDict = class(TDictionary<string, Integer>)
  protected
    procedure ValueNotify(const Value: Integer; Action: TCollectionNotification); override;
  end;

  TFoo = class
    Name: string;
  end;

  TFooList = class(TList<TFoo>)
  protected
    procedure Notify(const Item: TFoo; Action: TCollectionNotification); override;
  end;

  // Its constructor does not call the inherited one
  TBareList = class(TList<Integer>)
  protected
    procedure Notify(const Item: Integer; Action: TCollectionNotification); override;
  public
    constructor Create;
  end;

const
  ACTIONS: array[TCollectionNotification] of string = ('adding', 'removed');

procedure TLogList.Notify(const Item: Integer; Action: TCollectionNotification);
begin
  WriteLn('  ', ACTIONS[Action], ' ', Item);
  if (Action = TCollectionNotification.cnAdding) and (Item < 0) then
    raise EHook.Create('negative ' + IntToStr(Item));
  if (Action = TCollectionNotification.cnRemoved) and (Item = 13) then
    raise EHook.Create('unlucky ' + IntToStr(Item));
end;

procedure TLogStack.Notify(const Item: string; Action: TCollectionNotification);
begin
  WriteLn('  stack ', ACTIONS[Action], ' ', Item);
end;

procedure TLogQueue.Notify(const Item: string; Action: TCollectionNotification);
begin
  WriteLn('  queue ', ACTIONS[Action], ' ', Item);
end;

procedure TLogDict.KeyNotify(const Key: string; Action: TCollectionNotification);
begin
  WriteLn('  dict key ', ACTIONS[Action], ' ', Key);
end;

procedure TLogDict.ValueNotify(const Value: Integer; Action: TCollectionNotification);
begin
  WriteLn('  dict value ', ACTIONS[Action], ' ', Value);
end;

procedure TValueDict.ValueNotify(const Value: Integer; Action: TCollectionNotification);
begin
  WriteLn('  value ', ACTIONS[Action], ' ', Value);
end;

procedure TFooList.Notify(const Item: TFoo; Action: TCollectionNotification);
begin
  WriteLn('  foo ', ACTIONS[Action], ' ', Item.Name);
end;

constructor TBareList.Create;
begin
end;

procedure TBareList.Notify(const Item: Integer; Action: TCollectionNotification);
begin
  WriteLn('  bare ', ACTIONS[Action], ' ', Item);
end;

procedure Show(L: TList<Integer>);
var
  X: Integer;
begin
  Write('  [');
  for X in L do
    Write(' ', X);
  WriteLn(' ]');
end;

var
  L: TLogList;
  S: TLogStack;
  Q: TLogQueue;
  D: TLogDict;
  VD: TValueDict;
  FL: TFooList;
  A, C: TFoo;
  BL: TBareList;
  B: Boolean;
begin
  WriteLn('list: Create, Add, Insert, AddRange, InsertRange');
  L := TLogList.Create([1, 2]);
  L.Add(3);
  L.Insert(0, 0);
  L.AddRange([4, 5]);
  L.InsertRange(1, [7]);
  Show(L);

  WriteLn('list: cnAdding raises');
  try
    L.Add(-1);
  except
    on E: EHook do WriteLn('  ', E.Message);
  end;
  try
    L.AddRange([8, -2, 9]);
  except
    on E: EHook do WriteLn('  ', E.Message);
  end;
  Show(L);

  WriteLn('list: L[I] := X');
  L[0] := 10;

  WriteLn('list: a larger Count tells nothing');
  L.Count := L.Count + 1;
  Show(L);

  WriteLn('list: Extract, ExtractAt, Sort, Exchange, Move, Reverse, Capacity tell nothing');
  L.Extract(10);
  L.ExtractAt(0);
  L.Sort;
  L.Exchange(0, 1);
  L.Move(0, 2);
  L.Reverse;
  L.Capacity := 32;
  L.TrimExcess;
  Show(L);

  WriteLn('list: Delete, Remove');
  L.Delete(0);
  L.Remove(3);
  Show(L);

  WriteLn('list: Clear tells every item in index order, also after an exception');
  L.Clear;
  L.AddRange([12, 13, 14, 13]);
  try
    L.Clear;
  except
    on E: EHook do
    begin
      WriteLn('  ', E.Message, ', suppressed ', Length(E.Suppressed));
      if Length(E.Suppressed) > 0 then
        WriteLn('  ', E.Suppressed[0].Message);
    end;
  end;
  WriteLn('  count ', L.Count);
  L.Free;

  WriteLn('list of instances: writing the same instance removes nothing');
  A := TFoo.Create;
  A.Name := 'a';
  C := TFoo.Create;
  C.Name := 'c';
  FL := TFooList.Create;
  FL.Add(A);
  FL[0] := A;
  FL[0] := C;
  FL.Free;
  A.Free;
  C.Free;

  WriteLn('list whose constructor skips the inherited one');
  BL := TBareList.Create;
  BL.Add(1);
  BL.Free;

  WriteLn('stack');
  S := TLogStack.Create(['a']);
  S.Push('b');
  S.Push('c');
  WriteLn('  pop ', S.Pop);
  WriteLn('  extract ', S.Extract);
  S.Push('d');
  S.Clear;
  S.Push('e');
  WriteLn('  free');
  S.Free;

  WriteLn('queue');
  Q := TLogQueue.Create;
  Q.Enqueue('a');
  Q.Enqueue('b');
  Q.Enqueue('c');
  WriteLn('  dequeue ', Q.Dequeue);
  WriteLn('  free');
  Q.Free;

  WriteLn('dictionary');
  D := TLogDict.Create;
  D.Add('a', 1);
  B := D.TryAdd('b', 2);
  WriteLn('  TryAdd new ', B);
  B := D.TryAdd('a', 9);
  WriteLn('  TryAdd existing ', B);
  try
    D.Add('a', 9);
  except
    on E: EListError do WriteLn('  ', E.Message);
  end;
  D.AddOrSetValue('a', 3);
  D['c'] := 4;
  D['b'] := 5;
  D.ExtractPair('c');
  D.Remove('b');
  D.Add('d', 6);
  D.Clear;
  D.Add('e', 7);
  WriteLn('  free');
  D.Free;

  WriteLn('dictionary telling the values only');
  VD := TValueDict.Create;
  VD.Add('a', 1);
  VD['a'] := 2;
  VD.Remove('a');
  VD.Free;
end.

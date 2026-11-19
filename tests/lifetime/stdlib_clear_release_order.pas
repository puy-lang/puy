program StdlibClearReleaseOrder;

// Clear and freeing a list or a dictionary let go of the items in index (insertion) order, each
// item with what it holds before the next one, whether the slots are written one at a time (a
// list with few items for its capacity) or the whole array is let go of at once (a list that
// is more than a quarter full, freeing a container, a dictionary). While they are let go of,
// the container already holds no items: a destructor that looks at it sees Count = 0
// (collections.md 9.4; PR #304). No leak.

uses System.Generics.Collections;

type
  TItem = class(TRefObject)
    Name: string;
    Inner: TItem;
    constructor Create(const AName: string; AInner: TItem);
    destructor Destroy; override;
  end;

  // Prints the count of the container being emptied when it is destroyed.
  TWatcher = class(TRefObject)
    destructor Destroy; override;
  end;

var
  WatchedList: TList<TWatcher>;
  WatchedDict: TDictionary<Integer, TWatcher>;

constructor TItem.Create(const AName: string; AInner: TItem);
begin
  inherited Create;
  Name := AName;
  Inner := AInner;
end;

destructor TItem.Destroy;
begin
  WriteLn('  destroy ', Name);
  inherited;
end;

destructor TWatcher.Destroy;
begin
  if WatchedList <> nil then
    WriteLn('  watcher: list count ', WatchedList.Count)
  else
    WriteLn('  watcher: dictionary count ', WatchedDict.Count, ', has 1: ',
      WatchedDict.ContainsKey(1));
  inherited;
end;

// The temporaries of a call live until the end of the routine (lifetime.md), so the items are
// made in a routine of their own.
procedure AddNew(L: TList<TItem>; const Name: string);
begin
  L.Add(TItem.Create(Name, TItem.Create(Name + '.inner', nil)));
end;

procedure PutNew(D: TDictionary<Integer, TItem>; Key: Integer; const Name: string);
begin
  D.Add(Key, TItem.Create(Name, TItem.Create(Name + '.inner', nil)));
end;

procedure Lists;
var
  L: TList<TItem>;
begin
  L := TList<TItem>.Create;
  AddNew(L, 'a');
  AddNew(L, 'b');
  AddNew(L, 'c');
  WriteLn('Clear (3 of capacity 4)');
  L.Clear;
  WriteLn('Capacity ', L.Capacity, ', Count ', L.Count);
  L.Capacity := 16;
  AddNew(L, 'd');
  AddNew(L, 'e');
  WriteLn('Clear (2 of capacity 16)');
  L.Clear;
  WriteLn('Capacity ', L.Capacity, ', Count ', L.Count);
  AddNew(L, 'f');
  AddNew(L, 'g');
  WriteLn('Free');
  L.Free;
  WriteLn('end of Lists');
end;

procedure Dictionaries;
var
  D: TDictionary<Integer, TItem>;
begin
  D := TDictionary<Integer, TItem>.Create;
  PutNew(D, 3, 'h');
  PutNew(D, 1, 'i');
  WriteLn('Clear');
  D.Clear;
  WriteLn('Count ', D.Count);
  PutNew(D, 2, 'j');
  PutNew(D, 5, 'k');
  D.Remove(2);
  PutNew(D, 4, 'l');
  WriteLn('Free');
  D.Free;
  WriteLn('end of Dictionaries');
end;

procedure AddWatcher(L: TList<TWatcher>);
begin
  L.Add(TWatcher.Create);
end;

procedure PutWatcher(D: TDictionary<Integer, TWatcher>; Key: Integer);
begin
  D.Add(Key, TWatcher.Create);
end;

procedure Watched;
begin
  WatchedList := TList<TWatcher>.Create;
  AddWatcher(WatchedList);
  AddWatcher(WatchedList);
  AddWatcher(WatchedList);
  WriteLn('Clear (3 of capacity 4)');
  WatchedList.Clear;
  WatchedList.Capacity := 16;
  AddWatcher(WatchedList);
  WriteLn('Clear (1 of capacity 16)');
  WatchedList.Clear;
  WatchedList.Free;
  WatchedList := nil;
  WatchedDict := TDictionary<Integer, TWatcher>.Create;
  PutWatcher(WatchedDict, 1);
  PutWatcher(WatchedDict, 2);
  WriteLn('Clear');
  WatchedDict.Clear;
  WatchedDict.Free;
  WriteLn('end of Watched');
end;

begin
  Lists;
  Dictionaries;
  Watched;
end.

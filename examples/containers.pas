program Containers;

uses
  System.SysUtils, System.Generics.Collections, System.Generics.Defaults;

// The containers of System.Generics.Collections: TList<T> (adding, inserting, slices, searching,
// stable sorting with the default order or a comparer, binary search), TArray.Sort on a plain
// dynamic array, TDictionary<K, V> (enumerated in insertion order, Keys / Values, TryGetValue,
// a case-insensitive comparer), THashSet<T> with set operations, and TStack<T> / TQueue<T>.
// Changing a container inside its own for..in loop raises EInvalidOperation.

type
  TTask = record
    Name: string;
    Priority: Integer;
  end;

function Task(const Name: string; Priority: Integer): TTask;
begin
  Result := Default(TTask);
  Result.Name := Name;
  Result.Priority := Priority;
end;

function Show(const Values: TArray<Integer>): string; overload;
begin
  Result := '';
  for var V in Values do
  begin
    if Result <> '' then
      Result := Result + ' ';
    Result := Result + V.ToString;
  end;
  Result := '[' + Result + ']';
end;

function Show(const Values: TArray<string>): string; overload;
begin
  Result := '';
  for var V in Values do
  begin
    if Result <> '' then
      Result := Result + ' ';
    Result := Result + V;
  end;
  Result := '[' + Result + ']';
end;

procedure Lists;
var
  L: TList<Integer>;
  Tasks: TList<TTask>;
  Index: Integer;
begin
  WriteLn('TList<Integer>:');
  L := TList<Integer>.Create([42, 7, 19]);
  try
    L.Add(3);
    L.AddRange([25, 7]);
    L.Insert(0, 64);
    WriteLn('  ', Show(L.ToArray), ', count ', L.Count, ', first ', L.First, ', last ', L.Last);
    WriteLn('  L[2] = ', L[2], ', L[1..3] = ', Show(L[1..3]), ', L[4..] = ', Show(L[4..]));
    WriteLn('  IndexOf(7) = ', L.IndexOf(7), ', LastIndexOf(7) = ', L.LastIndexOf(7),
      ', Contains(8) = ', L.Contains(8));
    L.Remove(7);
    L.Delete(0);
    L[0] := L[0] * 2;
    WriteLn('  after Remove(7), Delete(0), L[0] * 2: ', Show(L.ToArray));
    L.Sort;
    WriteLn('  sorted: ', Show(L.ToArray));
    if L.BinarySearch(25, Index) then
      WriteLn('  25 is at ', Index);
    if not L.BinarySearch(20, Index) then
    begin
      L.Insert(Index, 20);
      WriteLn('  20 inserted at ', Index, ': ', Show(L.ToArray));
    end;
    L.Sort(TComparer<Integer>.Construct(
      function(const Left, Right: Integer): Integer
      begin
        // Reversed arguments give the descending order; KeyCompare cannot overflow
        // the way Right - Left can.
        Result := KeyCompare(Right, Left);
      end));
    WriteLn('  descending: ', Show(L.ToArray));
    try
      for var N in L do
        if N > 30 then
          L.Add(N div 2);
    except
      on E: EInvalidOperation do
        WriteLn('  changed during for..in: ', E.ClassName);
    end;
  finally
    L.Free;
  end;

  // The sort is stable: tasks with the same priority keep the order they were added in.
  WriteLn('TList<TTask> sorted by priority:');
  Tasks := TList<TTask>.Create;
  try
    Tasks.Add(Task('write docs', 2));
    Tasks.Add(Task('fix crash', 1));
    Tasks.Add(Task('review PR', 2));
    Tasks.Add(Task('release', 3));
    Tasks.Add(Task('triage', 1));
    Tasks.Sort(TComparer<TTask>.Construct(
      function(const Left, Right: TTask): Integer
      begin
        Result := KeyCompare(Left.Priority, Right.Priority);
      end));
    for var T in Tasks do
      WriteLn('  ', T.Priority, ' ', T.Name);
    // Records are compared field by field, so IndexOf finds an equal copy.
    WriteLn('  IndexOf(release, 3) = ', Tasks.IndexOf(Task('release', 3)));
  finally
    Tasks.Free;
  end;

  WriteLn('TArray.Sort:');
  var Names: TArray<string> := ['pear', 'fig', 'banana', 'kiwi', 'apple', 'plum'];
  TArray.Sort<string>(Names);
  WriteLn('  default order: ', Show(Names));
  TArray.Sort<string>(Names, TComparer<string>.Construct(
    function(const Left, Right: string): Integer
    begin
      Result := KeyCompare(Length(Left), Length(Right));
    end));
  WriteLn('  by length:     ', Show(Names));
end;

procedure Dictionaries;
var
  Counts: TDictionary<string, Integer>;
  Colors: TDictionary<string, string>;
  Count: Integer;
begin
  WriteLn('TDictionary<string, Integer> (word counts):');
  Counts := TDictionary<string, Integer>.Create;
  try
    var Words: TArray<string> := ['the', 'quick', 'brown', 'fox', 'jumps', 'over', 'the',
      'lazy', 'dog', 'the', 'fox'];
    for var Word in Words do
      if Counts.TryGetValue(Word, Count) then
        Counts[Word] := Count + 1
      else
        Counts.Add(Word, 1);
    // Enumeration follows the order the keys were first added in.
    Write(' ');
    for var Pair in Counts do
      Write(' ', Pair.Key, '=', Pair.Value);
    WriteLn;
    WriteLn('  ', Counts.Count, ' distinct words; "the" appears ', Counts['the'], ' times');
    Counts.Remove('the');
    Counts.AddOrSetValue('cat', 1);
    WriteLn('  keys: ', Show(Counts.Keys.ToArray));
    WriteLn('  ContainsKey(the) = ', Counts.ContainsKey('the'), ', ContainsValue(2) = ',
      Counts.ContainsValue(2));
    try
      WriteLn(Counts['cow']);
    except
      on E: EKeyNotFound do
        WriteLn('  ', E.ClassName, ': ', E.Message);
    end;
  finally
    Counts.Free;
  end;

  WriteLn('TDictionary with a case-insensitive comparer:');
  Colors := TDictionary<string, string>.Create(TStringComparer.OrdinalIgnoreCase);
  try
    Colors.Add('Red', '#ff0000');
    Colors.Add('Green', '#00ff00');
    Colors['RED'] := '#e00000';
    WriteLn('  red = ', Colors['red'], ', count ', Colors.Count, ', TryAdd(green) = ',
      Colors.TryAdd('green', '#000000'));
  finally
    Colors.Free;
  end;
end;

procedure Sets;
var
  Team, Online: THashSet<string>;
begin
  WriteLn('THashSet<string>:');
  Team := THashSet<string>.Create(['ann', 'bob', 'cid', 'dee']);
  Online := THashSet<string>.Create(['bob', 'eve', 'dee']);
  try
    WriteLn('  Add(ann) = ', Team.Add('ann'), ', Contains(cid) = ', Team.Contains('cid'));
    Team.IntersectWith(Online);
    WriteLn('  team members online: ', Show(Team.ToArray));
    Online.ExceptWith(Team);
    WriteLn('  online, not in the team: ', Show(Online.ToArray));
    Team.UnionWith(['fay', 'bob']);
    WriteLn('  after UnionWith: ', Show(Team.ToArray));
  finally
    Team.Free;
    Online.Free;
  end;
end;

procedure StacksAndQueues;
var
  Undo: TStack<string>;
  Jobs: TQueue<Integer>;
begin
  WriteLn('TStack<string> and TQueue<Integer>:');
  Undo := TStack<string>.Create;
  Jobs := TQueue<Integer>.Create([101, 102]);
  try
    Undo.Push('type "a"');
    Undo.Push('type "b"');
    Undo.Push('delete');
    Write('  undo:');
    while Undo.Count > 0 do
      Write(' ', Undo.Pop, ';');
    WriteLn;
    Jobs.Enqueue(103);
    WriteLn('  next job ', Jobs.Peek, ', ', Jobs.Count, ' waiting');
    Write(' ');
    while Jobs.Count > 0 do
      Write(' run ', Jobs.Dequeue, ';');
    WriteLn;
    try
      Undo.Pop;
    except
      on E: EListError do
        WriteLn('  ', E.ClassName, ': ', E.Message);
    end;
  finally
    Undo.Free;
    Jobs.Free;
  end;
end;

begin
  Lists;
  Dictionaries;
  Sets;
  StacksAndQueues;
end.

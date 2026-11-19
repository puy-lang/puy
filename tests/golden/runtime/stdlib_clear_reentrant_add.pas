program StdlibClearReentrantAdd;

// Clear lets go of the items while the container already holds none, and a destructor that
// adds items while that happens adds them to the emptied container: they stay after Clear, and
// the container grows for them as for any other Add, even past the capacity Clear keeps
// (collections.md 9.4). Before M4-134 a dictionary (and a set) shrank its arrays back to the
// old capacity after such adds, and a list did the same to its array, which left them broken.

uses System.Generics.Collections;

type
  // Adds three items to the container being cleared when it is destroyed (an item made by a
  // destructor, with Id 100 and up, adds nothing).
  TAdder = class(TRefObject)
    Id: Integer;
    constructor Create(AId: Integer);
    destructor Destroy; override;
  end;

var
  D: TDictionary<Integer, TAdder>;
  S: THashSet<TAdder>;
  L: TList<TAdder>;
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
    for I := 1 to 3 do
    begin
      Inc(Next);
      if D <> nil then
        D.Add(Next, nil)
      else if S <> nil then
        S.Add(TAdder.Create(Next))
      else
        L.Add(TAdder.Create(Next));
    end;
  inherited;
end;

procedure FillDictionary(Count: Integer);
var
  I: Integer;
begin
  for I := 1 to Count do
    D.Add(I, TAdder.Create(I));
end;

procedure ShowDictionary(const Title: string);
var
  P: TPair<Integer, TAdder>;
begin
  Write(Title, ': count ', D.Count, ', capacity ', D.Capacity, ', keys');
  for P in D do
    Write(' ', P.Key);
  WriteLn;
end;

procedure Dictionaries;
begin
  // 4 entries in a capacity of 4: the 12 entries added grow it past 4
  Next := 100;
  D := TDictionary<Integer, TAdder>.Create(4);
  FillDictionary(4);
  D.Clear;
  ShowDictionary('dictionary of capacity 4');
  WriteLn('has 104: ', D.ContainsKey(104), ', has 1: ', D.ContainsKey(1));
  D.Add(1, nil);
  D.Remove(101);
  ShowDictionary('after Add and Remove');
  D.Free;
  // 2 entries in a capacity of 16: the 6 entries added are kept in the capacity of 16
  Next := 200;
  D := TDictionary<Integer, TAdder>.Create(16);
  FillDictionary(2);
  D.Clear;
  ShowDictionary('dictionary of capacity 16');
  D.Free;
  D := nil;
end;

procedure FillSet;
var
  I: Integer;
begin
  for I := 1 to 4 do
    S.Add(TAdder.Create(I));
end;

procedure Sets;
var
  X: TAdder;
begin
  Next := 300;
  S := THashSet<TAdder>.Create;
  S.Capacity := 4;
  FillSet;
  S.Clear;
  Write('set: count ', S.Count, ', capacity ', S.Capacity, ', items');
  for X in S do
    Write(' ', X.Id);
  WriteLn;
  S.Free;
  S := nil;
end;

procedure FillList(Count: Integer);
var
  I: Integer;
begin
  for I := 1 to Count do
    L.Add(TAdder.Create(I));
end;

procedure ShowList(const Title: string);
var
  X: TAdder;
begin
  Write(Title, ': count ', L.Count, ', capacity ', L.Capacity, ', items');
  for X in L do
    Write(' ', X.Id);
  WriteLn;
end;

procedure Lists;
begin
  // 4 items in a capacity of 4: the whole array is let go of, and the 12 items added grow the
  // list past 4
  Next := 400;
  L := TList<TAdder>.Create;
  L.Capacity := 4;
  FillList(4);
  L.Clear;
  ShowList('list of capacity 4');
  L.Free;
  // 3 items in a capacity of 16: the slots are written one at a time, and the 9 items added
  // go into the slots already written, none of them written over
  Next := 500;
  L := TList<TAdder>.Create;
  L.Capacity := 16;
  FillList(3);
  L.Clear;
  ShowList('list of capacity 16');
  L.Free;
end;

begin
  Dictionaries;
  Sets;
  Lists;
end.

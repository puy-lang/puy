program StdlibOwningListFree;

// TObjectList<T> frees the items that leave it while OwnsObjects is True (collections.md 9.1):
// Delete, DeleteRange, Remove, a smaller Count, the item replaced by L[I] := X (but not when X is
// that item), Clear and freeing the list, in index order. Extract and ExtractAt hand the item
// over without freeing it, and nothing is freed while OwnsObjects is False. When a destructor
// raises, the rest of the items are still freed and the exceptions overlap into one (the last
// raised, with the earlier ones in Suppressed). The same instance put in twice is freed twice:
// the second Free raises EAccessViolation. Sort only reorders the items (none is freed), and
// the constructors taking a comparer own the items unless told otherwise.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TItem = class
  public
    Name: string;
    Fails: Boolean;
    constructor Create(const AName: string; AFails: Boolean = False);
    destructor Destroy; override;
  end;

constructor TItem.Create(const AName: string; AFails: Boolean);
begin
  Name := AName;
  Fails := AFails;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  if Fails then
    raise Exception.Create(Name + ' failed');
  inherited Destroy;
end;

procedure Report(E: Exception);
var
  S: Exception;
begin
  WriteLn(E.ClassName, ': ', E.Message);
  for S in E.Suppressed do
    WriteLn('  suppressed ', S.ClassName, ': ', S.Message);
end;

function ByName(const Left, Right: TItem): Integer;
begin
  Result := CompareStr(Left.Name, Right.Name);
end;

function Make(const Names: array of string): TObjectList<TItem>;
var
  I: Integer;
begin
  Result := TObjectList<TItem>.Create;
  for I := 0 to High(Names) do
    Result.Add(TItem.Create(Names[I]));
end;

var
  L: TObjectList<TItem>;
  X: TItem;

begin
  WriteLn('-- removing');
  L := Make(['a', 'b', 'c', 'd', 'e', 'f', 'g']);
  L.Delete(1);
  L.DeleteRange(1, 2);
  L.Remove(L[1]);
  L.Count := 2;
  WriteLn('count ', L.Count, ': ', L[0].Name, ' ', L[1].Name);
  L.Free;

  WriteLn('-- replacing');
  L := Make(['a', 'b']);
  L[0] := L[0];
  L[1] := TItem.Create('b2');
  WriteLn(L[0].Name, ' ', L[1].Name);
  L.Free;

  WriteLn('-- extracting');
  L := Make(['a', 'b', 'c']);
  X := L.Extract(L[1]);
  WriteLn('extracted ', X.Name);
  X.Free;
  X := L.ExtractAt(0);
  WriteLn('extracted ', X.Name);
  X.Free;
  WriteLn('count ', L.Count);
  L.Free;

  WriteLn('-- not owning');
  L := TObjectList<TItem>.Create(False);
  X := TItem.Create('x');
  L.Add(X);
  L.Add(X);
  L.Delete(0);
  L.Clear;
  WriteLn('owns ', L.OwnsObjects, ', count ', L.Count);
  L.OwnsObjects := True;
  L.Add(X);
  L.Free;

  WriteLn('-- a destructor raises');
  L := TObjectList<TItem>.Create;
  L.Add(TItem.Create('a', True));
  L.Add(TItem.Create('b'));
  L.Add(TItem.Create('c', True));
  L.Add(TItem.Create('d'));
  try
    L.Clear;
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', L.Count);
  L.Add(TItem.Create('e', True));
  L.Add(TItem.Create('f'));
  try
    L.Free;
  except
    on E: Exception do
      Report(E);
  end;

  WriteLn('-- sorting');
  L := TObjectList<TItem>.Create(TComparer<TItem>.Construct(ByName));
  L.Add(TItem.Create('c'));
  L.Add(TItem.Create('a'));
  L.Add(TItem.Create('b'));
  L.Sort;
  WriteLn('owns ', L.OwnsObjects);
  L.Free;
  L := TObjectList<TItem>.Create(TComparer<TItem>.Construct(ByName), False);
  X := TItem.Create('kept');
  L.Add(X);
  L.Sort;
  L.Free;
  X.Free;

  WriteLn('-- the same instance twice');
  L := TObjectList<TItem>.Create;
  X := TItem.Create('twice');
  L.Add(X);
  L.Add(X);
  try
    L.Clear;
  except
    on E: Exception do
      Report(E);
  end;
  L.Free;
end.

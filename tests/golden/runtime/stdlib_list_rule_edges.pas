program StdlibListRuleEdges;

// Rules of TList<T> not seen elsewhere (collections.md 3.1, 3.3, 4.1, 5.1-5.6, 10.3): a nil
// comparer given to Create sorts in the default order; the comparer given to Create is not used
// for equality (IndexOf stays exact); Extract finds by key equality (NaN equals NaN, -0.0 equals 0.0);
// LastIndexOf calls the equality comparer from the end; Exchange(I, I) and Sort of zero or one
// item still count as changes; a comparer that raises leaves the list as before the Sort;
// BinarySearch of an unsorted list raises nothing; a record item is copied out (changing the
// copy does not change the list); a user's TEnumerable<T> can be given to Create, AddRange,
// InsertRange and THashSet<T>.UnionWith.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TPoint = record
    X, Y: Integer;
  end;

  TDigits = class(TEnumerable<Integer>)
  public
    function ToArray: TArray<Integer>; override;
  end;

  EStop = class(Exception);

function TDigits.ToArray: TArray<Integer>;
begin
  Result := [7, 8, 9];
end;

procedure Show(const Title: string; L: TList<Integer>);
var
  X: Integer;
begin
  Write(Title, ':');
  for X in L do
    Write(' ', X);
  WriteLn;
end;

procedure ChangeWhileWalking(L: TList<Integer>; Kind: Integer);
var
  X: Integer;
begin
  try
    for X in L do
      case Kind of
        0: L.Exchange(0, 0);
        1: L.Sort;
      end;
    WriteLn('  no exception');
  except
    on E: EInvalidOperation do WriteLn('  ', E.Message);
  end;
end;

var
  L, L2: TList<Integer>;
  S: TList<string>;
  F: TList<Double>;
  P: TList<TPoint>;
  R: TPoint;
  H: THashSet<Integer>;
  D: TDigits;
  Calls: Integer;
  Found: Boolean;
  Index: Integer;
  NoComparer: IComparer<Integer>;
  Zero, NaN: Double;
begin
  Zero := 0.0;
  NaN := Zero / Zero;
  WriteLn('Create(nil) sorts in the default order');
  NoComparer := nil;
  L := TList<Integer>.Create(NoComparer);
  L.AddRange([3, 1, 2]);
  L.Sort;
  Show('  sorted', L);
  Found := L.BinarySearch(2, Index);
  WriteLn('  BinarySearch 2: ', Found, ' ', Index);
  L.Free;

  WriteLn('the comparer of Create is not used for equality');
  S := TList<string>.Create(TComparer<string>.Construct(
    function(const Left, Right: string): Integer
    begin
      Result := CompareText(Left, Right);
    end));
  S.AddRange(['banana', 'Apple']);
  S.Sort;
  WriteLn('  sorted: ', S[0], ' ', S[1]);
  WriteLn('  IndexOf(''APPLE'') ', S.IndexOf('APPLE'), ', BinarySearch(''APPLE'') ',
    S.BinarySearch('APPLE', Index), ' ', Index);
  S.Free;

  WriteLn('Extract finds by key equality');
  F := TList<Double>.Create([1.5, NaN, 0.0]);
  WriteLn('  ', F.Extract(NaN), ' count ', F.Count);
  WriteLn('  ', F.Extract(-0.0), ' count ', F.Count);
  F.Free;

  WriteLn('LastIndexOf asks the comparer from the end');
  S := TList<string>.Create(['a', 'bb', 'cc', 'd']);
  WriteLn('  ', S.LastIndexOf('xx', TEqualityComparer<string>.Construct(
    function(const Left, Right: string): Boolean
    begin
      WriteLn('  equals ', Left, ' ', Right);
      Result := Length(Left) = Length(Right);
    end,
    function(const Value: string): Int64
    begin
      Result := Length(Value);
    end)));
  S.Free;

  WriteLn('Exchange(I, I) and Sort of one item are changes');
  L := TList<Integer>.Create([5]);
  ChangeWhileWalking(L, 0);
  ChangeWhileWalking(L, 1);
  L.Free;

  WriteLn('a comparer that raises leaves the list as it was');
  L := TList<Integer>.Create([4, 2, 9, 1, 7, 3, 8, 6, 5, 0, 11, 10]);
  Calls := 0;
  try
    L.Sort(TComparer<Integer>.Construct(
      function(const Left, Right: Integer): Integer
      begin
        Inc(Calls);
        if Calls = 20 then
          raise EStop.Create('stop');
        Result := Left - Right;
      end));
  except
    on E: EStop do WriteLn('  ', E.Message);
  end;
  Show('  after', L);

  WriteLn('BinarySearch of an unsorted list raises nothing');
  Found := L.BinarySearch(9, Index);
  WriteLn('  ', (not Found) or (L[Index] = 9), ' ', (Index >= 0) and (Index <= L.Count));
  Found := L.BinarySearch(100, Index);
  WriteLn('  ', Found, ' ', (Index >= 0) and (Index <= L.Count));
  L.Free;

  WriteLn('a record item is copied out');
  P := TList<TPoint>.Create;
  R.X := 1;
  R.Y := 2;
  P.Add(R);
  R := P[0];
  R.X := 9;
  WriteLn('  ', P[0].X, ' ', R.X);
  P.Free;

  WriteLn('a user''s TEnumerable<T>');
  D := TDigits.Create;
  L := TList<Integer>.Create(D);
  Show('  Create', L);
  L.AddRange(D);
  L.InsertRange(1, D);
  Show('  AddRange, InsertRange', L);
  L2 := TList<Integer>.Create(L);
  WriteLn('  copy of a list: ', L2.Count);
  H := THashSet<Integer>.Create([1, 8]);
  H.UnionWith(D);
  Write('  UnionWith:');
  for Index in H do
    Write(' ', Index);
  WriteLn;
  H.Free;
  L2.Free;
  L.Free;
  D.Free;
end.

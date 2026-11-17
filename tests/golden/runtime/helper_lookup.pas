program HelperLookup;

// How `X.Name` finds a helper's member (objects.md 10.4-10.6, 10.8): two helpers for the same
// type used together (different names); `H.M(X)` naming the helper, also when the receiver is
// not a variable; the helper of an ancestor class working on a descendant (the nearest
// ancestor's first); a child helper with its parent's members; a receiver that is a constant
// or an expression, which a record helper's method gets as a copy; and a helper's property
// written through an instance and through a string variable. A helper's method uses the
// private members of an unrelated class of the same program, as any routine of it can.

type
  TOther = class
  private
    FSecret: Integer;
  end;

  TBase = class
  public
    Tag: string;
  end;

  TMid = class(TBase)
  end;

  TLeaf = class(TMid)
  end;

  TBaseHelper = class helper for TBase
    function Who: string;
    function Depth: Integer;
    function Peek(O: TOther): Integer;
    function GetLabel: string;
    procedure SetLabel(const V: string);
    property Caption: string read GetLabel write SetLabel;
  end;

  TMidHelper = class helper for TMid
    function Depth: Integer;
  end;

  TUpper = record helper for string
    function Shout: string;
  end;

  TCounter = record helper for string
    function Vowels: Integer;
    function GetFirst: Char;
    procedure SetFirst(C: Char);
    property First: Char read GetFirst write SetFirst;
  end;

  TIntBase = record helper for Integer
    function Half: Integer;
  end;

  TIntMore = record helper(TIntBase) for Integer
    function Quarter: Integer;
    procedure Reset;
  end;

function TBaseHelper.Who: string;
begin
  Result := 'base helper of ' + Tag;
end;

function TBaseHelper.Depth: Integer;
begin
  Result := 1;
end;

function TBaseHelper.Peek(O: TOther): Integer;
begin
  O.FSecret := 42;
  Result := O.FSecret;
end;

function TBaseHelper.GetLabel: string;
begin
  Result := '[' + Tag + ']';
end;

procedure TBaseHelper.SetLabel(const V: string);
begin
  Tag := V;
end;

function TCounter.GetFirst: Char;
begin
  Result := Self[1];
end;

procedure TCounter.SetFirst(C: Char);
begin
  Self[1] := C;
end;

function TMidHelper.Depth: Integer;
begin
  Result := 2;
end;

function TUpper.Shout: string;
begin
  Result := Self + '!';
end;

function TCounter.Vowels: Integer;
var
  C: Char;
begin
  Result := 0;
  for C in Self do
    if Pos(C, 'aeiou') > 0 then
      Inc(Result);
end;

function TIntBase.Half: Integer;
begin
  Result := Self div 2;
end;

function TIntMore.Quarter: Integer;
begin
  Result := Half div 2;
end;

procedure TIntMore.Reset;
begin
  Self := 0;
end;

const
  Big = 40;

var
  Other: TOther;
  Leaf: TLeaf;
  S: string;
  N: Integer;
begin
  Leaf := TLeaf.Create;
  Leaf.Tag := 'leaf';
  WriteLn(Leaf.Who);
  WriteLn(Leaf.Depth, ' ', TBaseHelper.Depth(Leaf), ' ', TBase(Leaf).Depth);
  Leaf.Caption := 'renamed';
  WriteLn(Leaf.Caption);
  Other := TOther.Create;
  WriteLn(Leaf.Peek(Other));
  Other.Free;
  Leaf.Free;

  S := 'banana';
  WriteLn(S.Shout, ' ', S.Vowels);
  WriteLn(TUpper.Shout(S), ' ', TCounter.Vowels('queue'));
  WriteLn(TUpper.Shout(S + S));
  S.First := 'B';
  WriteLn(S, ' ', S.First);

  N := 100;
  WriteLn(N.Half, ' ', N.Quarter, ' ', TIntBase.Half(N), ' ', TIntMore.Quarter(N));
  WriteLn(Big.Quarter, ' ', (N + 4).Half, ' ', TIntBase.Half(3 * 6));
  N.Reset;
  WriteLn(N);
  N := 5;
  TIntMore.Reset(N);
  WriteLn(N, ' ', Big);
end.

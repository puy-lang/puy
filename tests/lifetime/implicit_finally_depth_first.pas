program ImplicitFinallyDepthFirst;

// The implicit `finally` lets go of the managed registers in list order, and finishes the
// destruction of one instance (its destructor, then the instances its fields hold, in turn)
// before it goes on to the next register (design objects.md 3.8 B, lifetime.md 3.8). A temporary
// (lifetime.md 3.4) holds a reference too: an instance a temporary also holds is destroyed when
// that temporary is let go of, after the variables.

type
  TItem = class(TRefObject)
    Name: string;
    Child: TItem;
    constructor Create(const AName: string; AChild: TItem);
    destructor Destroy; override;
  end;

  // No destructor: its instances are destroyed without a frame, their fields let go of next
  TPlain = class(TRefObject)
    Item: TItem;
    constructor Create(AItem: TItem);
  end;

  TPair = record
    First: TItem;
    Second: TItem;
  end;

constructor TItem.Create(const AName: string; AChild: TItem);
begin
  Name := AName;
  Child := AChild;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

constructor TPlain.Create(AItem: TItem);
begin
  Item := AItem;
end;

// Makes an instance with no temporary of the caller holding it or its children
function Chain(const Name: string; Depth: Integer): TItem;
begin
  if Depth = 0 then
    Result := TItem.Create(Name, nil)
  else
    Result := TItem.Create(Name, Chain(Name + '.c', Depth - 1));
end;

function Plain(const Name: string): TPlain;
begin
  Result := TPlain.Create(Chain(Name, 1));
end;

function Pair(const A, B: string): TPair;
begin
  Result := Default(TPair);
  Result.First := Chain(A, 1);
  Result.Second := Chain(B, 0);
end;

function Items(const A, B: string): TArray<TItem>;
begin
  Result := [Chain(A, 1), Chain(B, 0)];
end;

procedure Locals;
var
  H: TItem;
  P: TPlain;
  R: TPair;
  A: TArray<TItem>;
  V: TItem;
begin
  H := Chain('h', 2);
  P := Plain('p');
  R := Pair('r1', 'r2');
  A := Items('a0', 'a1');
  V := Chain('v', 0);
  WriteLn('end of locals ', H.Name, P.Item.Name, R.First.Name, Length(A), V.Name);
end;

procedure Unwound;
var
  H: TItem;
  V: TItem;
begin
  H := Chain('h', 1);
  V := Chain('v', 0);
  WriteLn('raising ', H.Name, V.Name);
  raise Exception.Create('x');
end;

procedure Temporaries;
var
  H: TItem;
  V: TItem;
begin
  H := Chain('h', 0);
  // The right side is made in a temporary that keeps its reference until the routine ends
  H.Child := TItem.Create('field', nil);
  V := Chain('v', 0);
  WriteLn('end of temporaries ', H.Child.Name, V.Name);
end;

procedure Use(A, C: TItem);
begin
  WriteLn('use ', A.Name, A.Child.Name, C.Name);
end;

procedure NestedTemporaries;
var
  V: TItem;
begin
  // The temporary holding `a` comes before the one holding `b` in the list: `b` is destroyed
  // within `a`'s destruction, as the temporary holding it no longer holds the last reference
  Use(TItem.Create('a', TItem.Create('b', nil)), TItem.Create('c', nil));
  V := TItem.Create('v', nil);
  WriteLn('end of nested temporaries ', V.Name);
end;

begin
  Locals;
  try
    Unwound;
  except
    on E: Exception do
      WriteLn('caught ', E.Message);
  end;
  Temporaries;
  NestedTemporaries;
  WriteLn('end');
end.

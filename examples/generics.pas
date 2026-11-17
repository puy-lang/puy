program Generics;

uses
  System.SysUtils;

// Generic types and routines: a generic record and a generic class, routines whose type
// arguments are inferred from the arguments, the System procedural types TFunc / TPredicate /
// TProc, and constraints (a class type, an interface and `constructor`) that let a generic body
// call the members of its type parameter. Each instantiation is compiled for its own type
// arguments, so a TStack<Integer> holds plain integers.

type
  // A value that may be missing. Some / None build it; Default(T) is the empty value of any T.
  TMaybe<T> = record
  private
    FHasValue: Boolean;
    FValue: T;
  public
    class function Some(const AValue: T): TMaybe<T>; static;
    class function None: TMaybe<T>; static;
    function OrElse(const Fallback: T): T;
    property HasValue: Boolean read FHasValue;
  end;

  // A last-in, first-out stack over a dynamic array of T.
  TStack<T> = class
  private
    FItems: TArray<T>;
    function GetCount: Integer;
  public
    procedure Push(const Item: T);
    function Pop: TMaybe<T>;
    property Count: Integer read GetCount;
  end;

  IPriced = interface
    function Price: Currency;
    function Describe: string;
  end;

  TBook = class(TRefObject, IPriced)
  private
    FTitle: string;
    FPrice: Currency;
  public
    constructor Create(const ATitle: string; APrice: Currency);
    function Price: Currency;
    function Describe: string;
  end;

  TCoffee = class(TRefObject, IPriced)
  private
    FSize: string;
  public
    constructor Create(const ASize: string);
    function Price: Currency;
    function Describe: string;
  end;

  // An interface constraint: the body calls Price and Describe on values of T.
  TCart<T: IPriced> = class
  private
    FItems: TArray<T>;
  public
    procedure Add(const Item: T);
    function Total: Currency;
    procedure Print;
  end;

  TCounter = class
  private
    FValue: Integer;
  public
    constructor Create;
    procedure Bump;
    property Value: Integer read FValue;
  end;

class function TMaybe<T>.Some(const AValue: T): TMaybe<T>;
begin
  Result := Default(TMaybe<T>);
  Result.FHasValue := True;
  Result.FValue := AValue;
end;

class function TMaybe<T>.None: TMaybe<T>;
begin
  Result := Default(TMaybe<T>);
end;

function TMaybe<T>.OrElse(const Fallback: T): T;
begin
  if FHasValue then
    Result := FValue
  else
    Result := Fallback;
end;

function TStack<T>.GetCount: Integer;
begin
  Result := Length(FItems);
end;

procedure TStack<T>.Push(const Item: T);
begin
  FItems := FItems + [Item];
end;

function TStack<T>.Pop: TMaybe<T>;
begin
  if Length(FItems) = 0 then
    Exit(TMaybe<T>.None);
  Result := TMaybe<T>.Some(FItems[High(FItems)]);
  SetLength(FItems, Length(FItems) - 1);
end;

constructor TBook.Create(const ATitle: string; APrice: Currency);
begin
  inherited Create;
  FTitle := ATitle;
  FPrice := APrice;
end;

function TBook.Price: Currency;
begin
  Result := FPrice;
end;

function TBook.Describe: string;
begin
  Result := 'book "' + FTitle + '"';
end;

constructor TCoffee.Create(const ASize: string);
begin
  inherited Create;
  FSize := ASize;
end;

function TCoffee.Price: Currency;
begin
  if FSize = 'large' then
    Result := 4.2
  else
    Result := 3.5;
end;

function TCoffee.Describe: string;
begin
  Result := FSize + ' coffee';
end;

procedure TCart<T>.Add(const Item: T);
begin
  FItems := FItems + [Item];
end;

function TCart<T>.Total: Currency;
begin
  Result := 0;
  for var Item in FItems do
    Result := Result + Item.Price;
end;

procedure TCart<T>.Print;
begin
  for var Item in FItems do
    WriteLn('  ', Item.Describe, ': ', Item.Price);
  WriteLn('  total: ', Total);
end;

constructor TCounter.Create;
begin
  inherited Create;
  FValue := 100;
end;

procedure TCounter.Bump;
begin
  Inc(FValue);
end;

// Generic routines. The calls below leave out the type arguments: they are inferred from the
// arguments (the element type of the array, the parameter types of the anonymous method).

function Map<T, U>(const Items: TArray<T>; const F: TFunc<T, U>): TArray<U>;
begin
  SetLength(Result, Length(Items));
  for var I := 0 to High(Items) do
    Result[I] := F(Items[I]);
end;

function Filter<T>(const Items: TArray<T>; const Keep: TPredicate<T>): TArray<T>;
begin
  Result := nil;
  for var Item in Items do
    if Keep(Item) then
      Result := Result + [Item];
end;

function Fold<T, TAcc>(const Items: TArray<T>; const Start: TAcc;
  const Step: TFunc<TAcc, T, TAcc>): TAcc;
begin
  Result := Start;
  for var Item in Items do
    Result := Step(Result, Item);
end;

procedure ForEach<T>(const Items: TArray<T>; const Action: TProc<T>);
begin
  for var Item in Items do
    Action(Item);
end;

// KeyEquals compares two values of any type: by value for numbers, strings and records,
// by identity for classes that do not override Equals.
function CountOf<T>(const Items: TArray<T>; const Value: T): Integer;
begin
  Result := 0;
  for var Item in Items do
    if KeyEquals(Item, Value) then
      Inc(Result);
end;

procedure Swap<T>(var A, B: T);
begin
  var Tmp := A;
  A := B;
  B := Tmp;
end;

// The `constructor` and class constraints let the body create a T and call its methods.
function MakeAndBump<T: TCounter, constructor>(Times: Integer): T;
begin
  Result := T.Create;
  for var I := 1 to Times do
    Result.Bump;
end;

function Join(const Items: TArray<string>): string;
begin
  Result := '';
  for var S in Items do
  begin
    if Result <> '' then
      Result := Result + ', ';
    Result := Result + S;
  end;
end;

var
  Numbers: TStack<Integer>;
  Words: TStack<string>;
  Cart: TCart<IPriced>;
  Books: TCart<TBook>;
  Counter: TCounter;

begin
  WriteLn('generic class and record:');
  Numbers := TStack<Integer>.Create;
  Words := TStack<string>.Create;
  try
    for var I := 1 to 3 do
      Numbers.Push(I * 10);
    Words.Push('first');
    Words.Push('second');
    WriteLn('  ', Numbers.Count, ' numbers, ', Words.Count, ' words');
    Write(' ');
    while Numbers.Count > 0 do
      Write(' ', Numbers.Pop.OrElse(-1));
    WriteLn;
    WriteLn('  pop from empty: ', Numbers.Pop.OrElse(-1), ', has value: ',
      Numbers.Pop.HasValue);
    WriteLn('  ', Words.Pop.OrElse('?'), ' / ', Words.Pop.OrElse('?'), ' / ',
      Words.Pop.OrElse('?'));
  finally
    Numbers.Free;
    Words.Free;
  end;

  WriteLn('inferred type arguments:');
  var Prices: TArray<Double> := [12.5, 3.0, 8.25, 20.0];
  var Labels := Map(Prices,
    function(Price: Double): string
    begin
      Result := Format('$%.2f', [Price]);
    end);
  WriteLn('  map:    ', Join(Labels));
  var Cheap := Filter(Prices,
    function(Price: Double): Boolean
    begin
      Result := Price < 10;
    end);
  WriteLn('  filter: ', Length(Cheap), ' under $10');
  var Sum := Fold(Prices, 0.0,
    function(Acc: Double; Price: Double): Double
    begin
      Result := Acc + Price;
    end);
  WriteLn('  fold:   sum = ', Sum:0:2);
  var Longest := Fold(['pear', 'fig', 'banana', 'kiwi'], '',
    function(Best: string; Word: string): string
    begin
      if Length(Word) > Length(Best) then
        Result := Word
      else
        Result := Best;
    end);
  WriteLn('  fold:   longest = ', Longest);
  Write('  each:  ');
  ForEach(['a', 'b', 'c'],
    procedure(Item: string)
    begin
      Write(' <', Item, '>');
    end);
  WriteLn;
  WriteLn('  count:  ', CountOf([3, 1, 3, 3, 2], 3), ' threes, ',
    CountOf(['x', 'y', 'x'], 'x'), ' x');
  var A := 'left';
  var B := 'right';
  Swap(A, B);
  WriteLn('  swap:   ', A, ' ', B);

  WriteLn('constraints:');
  Cart := TCart<IPriced>.Create;
  Books := TCart<TBook>.Create;
  try
    Cart.Add(TBook.Create('Generics in Practice', 29.9));
    Cart.Add(TCoffee.Create('large'));
    Cart.Add(TCoffee.Create('small'));
    Cart.Print;
    Books.Add(TBook.Create('Pascal Patterns', 18.5));
    Books.Print;
  finally
    Cart.Free;
    Books.Free;
  end;
  Counter := MakeAndBump<TCounter>(3);
  WriteLn('  counter: ', Counter.Value);
  Counter.Free;
end.

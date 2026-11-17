program Closures;

uses
  System.SysUtils;

// Procedural types, method references (of object) and anonymous methods.
// An anonymous method captures the variables it uses: the variable itself is
// shared, so a counter keeps its value between calls and after the routine
// that declared it has returned.

type
  TIntFunc = reference to function(X: Integer): Integer;
  TCounter = reference to function: Integer;
  TCompare = function(A, B: Integer): Boolean;
  TNotifyEvent = procedure(Sender: TObject) of object;

  TButton = class
  private
    FCaption: string;
    FOnClick: TNotifyEvent;
  public
    constructor Create(const ACaption: string);
    procedure Click;
    property Caption: string read FCaption;
    property OnClick: TNotifyEvent read FOnClick write FOnClick;
  end;

  TForm = class
  private
    FClicks: Integer;
  public
    procedure ButtonClicked(Sender: TObject);
    property Clicks: Integer read FClicks;
  end;

constructor TButton.Create(const ACaption: string);
begin
  inherited Create;
  FCaption := ACaption;
end;

procedure TButton.Click;
begin
  if Assigned(FOnClick) then
    FOnClick(Self);
end;

procedure TForm.ButtonClicked(Sender: TObject);
begin
  Inc(FClicks);
  WriteLn('  clicked ', (Sender as TButton).Caption, ' (', FClicks, ')');
end;

// A plain routine can be the value of a procedural type.
function Ascending(A, B: Integer): Boolean;
begin
  Result := A < B;
end;

function Descending(A, B: Integer): Boolean;
begin
  Result := A > B;
end;

procedure Sort(var Items: TArray<Integer>; Before: TCompare);
begin
  for var I := 1 to High(Items) do
  begin
    var J := I;
    var Item := Items[I];
    while (J > 0) and Before(Item, Items[J - 1]) do
    begin
      Items[J] := Items[J - 1];
      Dec(J);
    end;
    Items[J] := Item;
  end;
end;

function Join(const Items: TArray<Integer>): string;
begin
  Result := '';
  for var N in Items do
  begin
    if Result <> '' then
      Result := Result + ' ';
    Result := Result + IntToStr(N);
  end;
end;

function Map(const Items: TArray<Integer>; F: TIntFunc): TArray<Integer>;
begin
  SetLength(Result, Length(Items));
  for var I := 0 to High(Items) do
    Result[I] := F(Items[I]);
end;

// Each call makes a new Count variable; the returned function keeps it alive.
function MakeCounter(Start: Integer): TCounter;
var
  Count: Integer;
begin
  Count := Start;
  Result := function: Integer
    begin
      Result := Count;
      Inc(Count);
    end;
end;

// Captured parameters work the same way.
function Adder(N: Integer): TIntFunc;
begin
  Result := function(X: Integer): Integer
    begin
      Result := X + N;
    end;
end;

function Compose(F, G: TIntFunc): TIntFunc;
begin
  Result := function(X: Integer): Integer
    begin
      Result := G(F(X));
    end;
end;

// A nested routine can also be returned as a reference to value.
function Multiplier(Factor: Integer): TIntFunc;

  function Times(X: Integer): Integer;
  begin
    Result := X * Factor;
  end;

begin
  Result := Times;
end;

var
  Numbers: TArray<Integer>;
  Next, Other: TCounter;
  Actions: TArray<TProc>;
  Total: Integer;
  Form: TForm;
  OK, Cancel: TButton;

begin
  WriteLn('procedural types:');
  Numbers := [5, 3, 8, 1, 9, 2];
  Sort(Numbers, Ascending);
  WriteLn('  ascending  ', Join(Numbers));
  Sort(Numbers, Descending);
  WriteLn('  descending ', Join(Numbers));

  WriteLn('anonymous methods:');
  WriteLn('  squares    ', Join(Map(Numbers,
    function(X: Integer): Integer
    begin
      Result := X * X;
    end)));
  WriteLn('  plus 10    ', Join(Map(Numbers, Adder(10))));
  WriteLn('  times 3    ', Join(Map(Numbers, Multiplier(3))));
  WriteLn('  (x+1)*2    ', Join(Map(Numbers, Compose(Adder(1), Multiplier(2)))));

  WriteLn('counters:');
  Next := MakeCounter(1);
  Other := MakeCounter(100);
  WriteLn('  ', Next(), ' ', Next(), ' ', Next(), ' / ', Other(), ' ', Other());

  // A variable declared with for var is new on each pass, so each action
  // remembers its own I. Total is one variable shared by all of them.
  WriteLn('captured variables:');
  Total := 0;
  for var I := 1 to 3 do
    Actions := Actions + [procedure
      begin
        Total := Total + I;
        WriteLn('  action ', I, ', total ', Total);
      end];
  for var Action in Actions do
    Action();
  WriteLn('  total after the loop: ', Total);

  // A method reference holds the object and the method together.
  WriteLn('events:');
  Form := TForm.Create;
  OK := TButton.Create('OK');
  Cancel := TButton.Create('Cancel');
  try
    OK.OnClick := Form.ButtonClicked;
    Cancel.OnClick := Form.ButtonClicked;
    OK.Click;
    Cancel.Click;
    OK.Click;
    Cancel.OnClick := nil;
    Cancel.Click;
    WriteLn('  ', Form.Clicks, ' clicks');
  finally
    Cancel.Free;
    OK.Free;
    Form.Free;
  end;
end.

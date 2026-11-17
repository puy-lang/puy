program ForInEnumerators;

// `for..in` with `GetEnumerator` (objects.md 13.1, 13.2): a class whose enumerator is a class
// managed by hand (destroyed after the loop, also after `Break`), with `Current` as a property;
// a record whose enumerator is a record (its `MoveNext` changes the hidden variable, so the
// change stays for the next iteration); `GetEnumerator` called once
// per loop. (FPC 3.2.2 has no `for var` and takes `Current` only as a property; those are in
// golden/runtime/for_in_enum_forms.)

type
  TRangeEnum = class
  private
    FCur, FLast: Integer;
    function GetCurrent: Integer;
  public
    constructor Create(First, Last: Integer);
    destructor Destroy; override;
    function MoveNext: Boolean;
    property Current: Integer read GetCurrent;
  end;

  TRange = class
  public
    First, Last: Integer;
    function GetEnumerator: TRangeEnum;
  end;

  TWordEnum = record
    Text: string;
    Pos: Integer;
    Word: string;
    function GetCurrent: string;
    function MoveNext: Boolean;
    property Current: string read GetCurrent;
  end;

  TWords = record
    Text: string;
    function GetEnumerator: TWordEnum;
  end;

var
  Calls: Integer;

constructor TRangeEnum.Create(First, Last: Integer);
begin
  inherited Create;
  FCur := First - 1;
  FLast := Last;
  WriteLn('enumerator created');
end;

destructor TRangeEnum.Destroy;
begin
  WriteLn('enumerator destroyed at ', FCur);
  inherited;
end;

function TRangeEnum.GetCurrent: Integer;
begin
  Result := FCur;
end;

function TRangeEnum.MoveNext: Boolean;
begin
  Inc(FCur);
  Result := FCur <= FLast;
end;

function TRange.GetEnumerator: TRangeEnum;
begin
  Inc(Calls);
  Result := TRangeEnum.Create(First, Last);
end;

function TWordEnum.MoveNext: Boolean;
var
  Start: Integer;
begin
  while (Pos <= Length(Text)) and (Text[Pos] = ' ') do
    Inc(Pos);
  Start := Pos;
  while (Pos <= Length(Text)) and (Text[Pos] <> ' ') do
    Inc(Pos);
  Word := Copy(Text, Start, Pos - Start);
  Result := Word <> '';
end;

function TWordEnum.GetCurrent: string;
begin
  Result := '<' + Word + '>';
end;

function TWords.GetEnumerator: TWordEnum;
begin
  Result := Default(TWordEnum);
  Result.Text := Text;
  Result.Pos := 1;
end;

var
  R: TRange;
  W: TWords;
  I, J, Sum: Integer;
  S: string;
begin
  R := TRange.Create;
  R.First := 3;
  R.Last := 6;
  Sum := 0;
  for I in R do
  begin
    WriteLn('I = ', I);
    Sum := Sum + I;
  end;
  WriteLn('sum ', Sum, ', calls ', Calls);
  for J in R do
  begin
    if J = 5 then
      Break;
    WriteLn('J = ', J);
  end;
  R.Last := 0;
  for I in R do
    WriteLn('never');
  WriteLn('calls ', Calls);
  W.Text := '  one two   three ';
  for S in W do
    WriteLn(S);
  R.Free;
end.

program Reflection;

uses
  System.SysUtils, System.Rtti, System.TypInfo;

type
  TCommandAttribute = class(TCustomAttribute)
  public
    LabelText: string;
    constructor Create(const ALabel: string);
  end;

  TCounter = class
  private
    FValue: Integer;
  public
    [TCommand('add to counter')]
    function Add(Amount: Integer): Integer;
  published
    property Value: Integer read FValue write FValue;
  end;

constructor TCommandAttribute.Create(const ALabel: string);
begin
  inherited Create;
  LabelText := ALabel;
end;

function TCounter.Add(Amount: Integer): Integer;
begin
  Inc(FValue, Amount);
  Result := FValue;
end;

function Describe(const Pattern: string; const Args: array of const): string;
begin
  // The caller's arguments are boxed as TValue and can be forwarded to Format.
  Result := Format(Pattern, Args);
end;

begin
  var Context := TRttiContext.Create;
  var CounterType := Context.GetType(TypeInfo(TCounter));
  var Counter := TCounter.Create;
  try
    var ValueProp := CounterType.GetProperty('Value');
    ValueProp.SetValue(Counter, TValue.From<Integer>(10));
    if ValueProp.Visibility = System.TypInfo.TMemberVisibility.mvPublished then
      WriteLn(Describe('Published %s = %d', [ValueProp.Name, ValueProp.GetValue(Counter)]));

    var Command := CounterType.GetMethod('Add');
    // RTTI owns the attribute instance returned by GetAttribute.
    var Tag := Command.GetAttribute<TCommandAttribute>;
    var Answer := Command.Invoke(Counter, [5]);
    WriteLn(Describe('%s returned %d', [Tag.LabelText, Answer.AsInteger]));
  finally
    Counter.Free;
  end;
end.

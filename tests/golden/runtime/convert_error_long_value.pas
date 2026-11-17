// The message of an EConvertError quotes at most the first 100 scalar values of the value and
// adds `…` when it cuts; Value keeps the whole value (docs/spec/semantics.md 7.1).
program ConvertErrorLongValue;

uses System.SysUtils;

var
  I: Integer;
  B: Boolean;
  F: Double;

procedure Check(const S: string);
begin
  try
    I := StrToInt(S);
  except
    on E: EConvertError do
      WriteLn(Length(S), ': ', E.Message, ' | Value length ', Length(E.Value));
  end;
end;

begin
  Check(StringOfChar('a', 100));
  Check(StringOfChar('a', 101));
  // Non-ASCII: a surrogate pair is one scalar value (two code units)
  Check(StringOfChar('あ', 99) + '😀' + 'z');
  Check(StringOfChar('あ', 99) + '😀');
  try
    B := StrToBool(StringOfChar('b', 150));
  except
    on E: EConvertError do WriteLn(E.Message);
  end;
  try
    F := StrToFloat(StringOfChar('f', 150));
  except
    on E: EConvertError do WriteLn(E.Message);
  end;
  try
    WriteLn(Format(StringOfChar('-', 120) + '%d', ['x']));
  except
    on E: EConvertError do WriteLn(E.Message, ' | Value length ', Length(E.Value));
  end;
end.

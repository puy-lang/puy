program SysUtilsErrors;

// Run-time errors of the System.SysUtils built-ins (semantics.md 5.3): a format that does not
// match its arguments is EConvertError with the format in the message and in `Value`, a result
// over the string length limit is EOutOfMemory (a precision only where it adds digits), and
// StrToBool of other text is EConvertError (an integer of any size is read).

uses System.SysUtils;

procedure Check(const Fmt: string; N: Integer);
begin
  try
    if N = 0 then
      WriteLn(Format(Fmt, []))
    else if N = 1 then
      WriteLn(Format(Fmt, [1]))
    else
      WriteLn(Format(Fmt, ['a', 1.5, True]));
  except
    on E: EConvertError do
      WriteLn(E.ClassName, ': ', E.Message, ' [', E.Value, ']');
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

var
  U: UTF8String;
begin
  Check('%d', 0);       // no argument
  Check('%d %d', 1);    // too few
  Check('%s', 1);       // `s` of an integer
  Check('%d', 2);       // `d` of a string
  Check('%f', 1);       // `f` of an integer
  Check('%1:s %2:s', 2); // `s` of a real
  Check('%2:s', 2);     // `s` of a Boolean
  Check('%m', 1);       // currency is not supported
  Check('%p', 1);       // nor pointers
  Check('%', 1);        // no kind
  Check('%5', 1);
  Check('%*d', 2);      // `*` of a string
  Check('%-1:d', 1);    // the index comes first
  Check('%3000000000d', 1); // over the length limit
  Check('%.2147483600d', 1); // a precision that adds digits over the limit
  Check('%.2147483600s', 2); // the precision of `s` only shortens
  Check('%.2147483600z', 1); // a wrong kind is EConvertError even with a large precision
  WriteLn(Format('%.2147483600g', [0.1]));
  Check('ok %%', 0);
  U := 'ü';
  WriteLn(Format('[%s] [%3s]', [U, U]));
  try
    WriteLn(StrToBool('yes'));
  except
    on E: EConvertError do
      WriteLn(E.ClassName, ': ', E.Message, ' [', E.Value, ']');
  end;
  WriteLn(StrToBool(' 1'), ' ', StrToBool('$0'), ' ', StrToBool('True'));
  WriteLn(StrToBool('99999999999999999999999999999'), ' ', StrToBool('-0000000000000000000000000000'));
end.

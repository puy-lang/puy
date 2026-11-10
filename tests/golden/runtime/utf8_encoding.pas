program Utf8Encoding;

// `UTF8String` (types.md 8.5): a lone surrogate becomes U+FFFD on the way from `string`;
// `Copy` and `Pos` work in bytes; `SetLength` grows with #0 bytes; a piece that starts or ends
// inside a multibyte character raises `EEncodingError` (`Copy` at either end, `SetLength`
// shrinking into a character); a byte index out of range is `ERangeError`; a `case` compares
// the value as a `string`; a `UTF8String` field of a record constant (a kind 7 constant inside an
// aggregate) is a `UTF8String`. The last `EEncodingError` is uncaught.

type
  TEntry = record
    Key: UTF8String;
    N: Integer;
  end;

const
  Entry: TEntry = (Key: 'é日'; N: 1);

var
  U: UTF8String;
  S: string;
  I: Integer;
  Words: TArray<UTF8String>;
  R: TEntry;

procedure Show(const Name: string; const V: UTF8String);
begin
  WriteLn(Name, ': [', V, '] ', Length(V));
end;

procedure Attempt(Step: Integer);
begin
  try
    case Step of
      1: Show('copy start', Copy(U, 3, 2));
      2: Show('copy end', Copy(U, 1, 2));
      3: SetLength(U, 4);
      4: WriteLn(U[0]);
      5: WriteLn(U[Length(U) + 1]);
    end;
  except
    on E: EEncodingError do WriteLn(Step, ' ', E.ClassName, ': ', E.Message);
    on E: ERangeError do WriteLn(Step, ' ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  S := 'a' + #$D800 + 'b' + #$DC00;
  U := S;
  Show('lone', U);
  for I := 1 to Length(U) do
    Write(U[I], ' ');
  WriteLn;
  S := U;
  WriteLn(Ord(S[2]), ' ', Length(S));
  U := 'aé日';
  Show('copy', Copy(U, 2, 2));
  Show('whole', Copy(U, 1, 100));
  Show('empty', Copy(U, 7, 1));
  WriteLn(Pos('日', U), ' ', Pos(UTF8String('x'), U), ' ', Pos('', U));
  for I := 1 to 5 do
    Attempt(I);
  SetLength(U, 7);
  Show('grown', U);
  WriteLn(U[7]);
  SetLength(U, 3);
  Show('shrunk', U);
  Words := ['é', 'x', 'other'];
  for U in Words do
    case U of
      'é': WriteLn('e acute');
      'x', 'y': WriteLn('x or y');
    else
      WriteLn('else ', U);
    end;
  R := Entry;
  R.Key := R.Key + '!';
  Show('field', R.Key);
  Show('constant', Entry.Key);
  U := 'é';
  WriteLn(Copy(U, 2, 1));
end.

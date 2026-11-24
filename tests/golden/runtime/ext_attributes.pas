// puy-test: test-ext
// The declarations the attributes of an extension generate (`docs/spec/extensions.md` 7):
// constants, an enumeration with a renamed value, a property from a getter and a setter, an
// exception whose parent is a built-in class, `UCS4Char` for Rust's `char`, and the range checks
// of `UCS4Char` and `Char` arguments.
program ExtAttributes;

uses TestExt;

var
  C: TCounter;
  K: TColor;
begin
  WriteLn(TestLimit, ' ', TestGreeting);
  K := TColor.Red;
  WriteLn(NextColor(K), ' ', NextColor(TColor.Green), ' ', NextColor(TColor.Azure));
  WriteLn(NextScalar($41), ' ', NextScalar($1F5FF));
  C := TCounter.Create(5);
  C.Count := C.Count + 2;
  WriteLn(C.Count, ' ', C.Value);
  try
    FailTestRange;
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    NextScalar($D800);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    NextChar(#$D800);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.

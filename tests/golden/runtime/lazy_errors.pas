program LazyErrors;

// The errors of TLazy<T> (docs/spec/stdlib/Puy.Lazy/README.md 3): an exception of the function
// reaches the reader and nothing is kept, so the next read calls the function again; reading
// Value or calling Reset while the value is being made, a nil function, and Reset of a TLazy
// made by CreateValue raise.

uses
  System.SysUtils, Puy.Lazy;

var
  Calls: Integer;
  L, Again: TLazy<Integer>;
  F: TFunc<Integer>;

procedure Check(const What: string; P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  Calls := 0;
  L := TLazy<Integer>.Create(
    function: Integer
    begin
      Inc(Calls);
      if Calls < 3 then
        raise EConvertError.Create('attempt ' + IntToStr(Calls) + ' failed');
      Result := Calls * 10;
    end);
  Check('first read', procedure begin WriteLn(L.Value) end);
  WriteLn('made: ', L.IsValueCreated);
  Check('second read', procedure begin WriteLn(L.Value) end);
  Check('third read', procedure begin WriteLn(L.Value) end);
  Check('fourth read', procedure begin WriteLn(L.Value) end);
  WriteLn('calls: ', Calls);

  // The function reads the value it is making
  Again := TLazy<Integer>.Create(
    function: Integer
    begin
      Result := Again.Value + 1;
    end);
  Check('read while making', procedure begin WriteLn(Again.Value) end);
  WriteLn('made: ', Again.IsValueCreated);

  Again := TLazy<Integer>.Create(
    function: Integer
    begin
      Again.Reset;
      Result := 1;
    end);
  Check('Reset while making', procedure begin WriteLn(Again.Value) end);

  F := nil;
  Check('nil function', procedure begin TLazy<Integer>.Create(F) end);

  L := TLazy<Integer>.CreateValue(7);
  L.Reset;
  WriteLn('made: ', L.IsValueCreated);
  Check('read after Reset of CreateValue', procedure begin WriteLn(L.Value) end);
end.

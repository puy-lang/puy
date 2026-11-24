program RandomErrors;

// The arguments of TRandom that make an empty range raise EArgumentException and leave the
// sequence where it was (docs/spec/stdlib/Puy.Random/README.md 3.4).

uses
  System.SysUtils, Puy.Random;

var
  R, R2: TRandom;

procedure Check(const What: string; P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: EArgumentException do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  R := TRandom.Create(5);
  Check('Next(0)', procedure begin R.Next(0) end);
  Check('Next(-1)', procedure begin R.Next(-1) end);
  Check('Next(5, 5)', procedure begin R.Next(5, 5) end);
  Check('Next(6, 5)', procedure begin R.Next(6, 5) end);
  Check('NextInt64(0)', procedure begin R.NextInt64(0) end);
  Check('NextInt64(High, Low)',
    procedure begin R.NextInt64(High(Int64), Low(Int64)) end);
  Check('NextBytes(-1)', procedure begin R.NextBytes(-1) end);
  Check('Choice([])', procedure
    var
      Empty: TArray<Integer>;
    begin
      Empty := nil;
      R.Choice<Integer>(Empty)
    end);
  // None of them took a value
  R2 := TRandom.Create(5);
  WriteLn(R.NextUInt64 = R2.NextUInt64);
end.

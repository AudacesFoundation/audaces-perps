import yaml
import matplotlib.pyplot as plt
import math
from jsonmerge import merge
from datetime import datetime
import plotly as ply
import pandas as pd
import plotly.express as px

TRANSFORM = False
PLOT_MEMORY = True
NB_INSTRUCTIONS = 1000

f_value_props = {
    # [Color, MinOffset, MaxOffset]
    "total_collateral": ["", 0, 1],
    "total_fee_balance": ["", 0, 1],
    "rebalancing_funds": ["#99cc99", 0, 0.5],  #
    # "rebalanced_v_coin": ["", 0, 1],
    "v_coin_amount": ["", 0, 1],
    "v_pc_amount": ["", 0, 1],
    "open_shorts_v_coin": ["", 0, 1],
    "open_longs_v_coin": ["", 0, 1],    #
    "insurance_fund": ["#808080", 0.2, 1.2],
    "market_price": ["#008080", 0.5, 1.5],
    "oracle_price": ["#99cc99", 0.5, 1.5],
    "equilibrium_price": ["#ff8000", 0.5, 1],  #

    # "signer_nonce",
    # "market_symbol",
    # "oracle_address",
    # "admin_address",
    # "vault_address",
    # "quote_decimals",
    # "coin_decimals",
    # "total_user_balances",
    # "last_funding_timestamp",
    # "last_recording_timestamp",
    # "funding_samples_offset",
    # "funding_samples",
    # "funding_history_offset",
    # "funding_history",
    # "funding_balancing_factors",
    # "number_of_instances",
}

m_value_props = {
    "gc_list_lengths",
    "page_full_ratios",
    "longs_depths",
    "shorts_depths"
}


market_state_line_header = "INFO - MarketDataPoint"

date_time = datetime.now().strftime("%d-%m-%Y_%H-%M-%S")
infile = open("../log/output.log")
outfile = open(
    "../log/formatted_output_{}.log".format(date_time), "a")
market_data_json = []
for line in infile:
    if (market_state_line_header in line) or ("DEBUG - Program" in line) or ("DEBUG - tx error:" in line) or ("INFO - Tree:" in line) or ("INFO - Initial Conditions:" in line) or ("INFO - Seed for this run:" in line):
        outfile.write(line)
    if market_state_line_header in line:
        market_state_datapoint_str = line[len(
            market_state_line_header):].replace("Instance", "").replace("PageInfo", "")  # Stripping header
        line_json = yaml.load(market_state_datapoint_str)
        market_data_json.append(line_json)

# Extract
market_data = {}
value_names = list(f_value_props.keys())
for key in market_data_json[0]:
    if key in value_names:
        market_data[key] = [data_point[key] for data_point in market_data_json]

# Normalize
if TRANSFORM:
    max_per_value = [max(market_data[key]) for key in value_names]
    min_per_value = [min(market_data[key]) for key in value_names]
    max_per_value[value_names.index(
        "market_price")] = max_per_value[value_names.index("oracle_price")]
    min_per_value[value_names.index(
        "market_price")] = min_per_value[value_names.index("oracle_price")]
    scaled_market_data = [[((1 - f_value_props[value_names[i]][1]) * (data_value_point - min_per_value[i]) / abs((max_per_value[i] / f_value_props[value_names[i]][2]) - min_per_value[i])) + f_value_props[value_names[i]][1] for data_value_point in market_data[value_names[i]]]
                          for i in range(len(value_names))]

else:
    max_per_value = [max(market_data[key]) for key in value_names]
    total_max = max(max_per_value)
    scaling_factors = [int(round(math.log10(total_max / value_max)))
                       if value_max != 0 else 1 for value_max in max_per_value]
    scaled_market_data = [[(10 ** scaling_factors[i]) * data_value_point for data_value_point in market_data[value_names[i]]]
                          for i in range(len(value_names))]


# Plotting
if PLOT_MEMORY:
    nb_lines = min(len(market_data_json), NB_INSTRUCTIONS)
    df = pd.DataFrame(market_data_json)
    print(df.columns)
    print(df.shape)
    df["shorts_depths"] = [k[0] for k in df["shorts_depths"]]
    df["longs_depths"] = [k[0] for k in df["longs_depths"]]
    df["gc_list_lengths"] = [k[0] for k in df["gc_list_lengths"]]
    for k in range(len(df["page_full_ratios"][0][0])):
        df[f"page_{k}_full_ratio"] = [l[0][k] for l in df["page_full_ratios"]]
    df.drop("page_full_ratios", axis=1)
    df = df.stack().reset_index()
    print(df)

    fig = px.line(df, x="level_0", y=0, color="level_1")
    fig.show()

    # print([len(m["page_full_ratios"]) for m in market_data_json])
    page_full_ratios = [
        market_data_json[i]["page_full_ratios"][0] for i in range(nb_lines)]
    longs_depths = [
        market_data_json[i]["longs_depths"] for i in range(nb_lines)
    ]
    shorts_depths = [
        market_data_json[i]["shorts_depths"] for i in range(nb_lines)
    ]

    for k in range(len(market_data_json[0]["page_full_ratios"][0])):
        plt.plot([page_full_ratios[i][k] for i in range(nb_lines)], label=(
            "page_full_ratios for page " + str(k)))
        plt.plot()
    gc_list_lenghts = [
        market_data_json[i]["gc_list_lengths"][0] for i in range(nb_lines)]  # TODO Mult instances
    # plt.plot([gc_list_lenghts[i] for i in range(nb_lines)], label=(
    #     "gc_list_length"))
    plt.plot(longs_depths, label=("longs_depths"))
    plt.plot(shorts_depths, label=("shorts_depths"))
elif TRANSFORM:
    for (i, key) in enumerate(value_names):
        if f_value_props[key][0] != "":
            plt.plot(scaled_market_data[i][:NB_INSTRUCTIONS], label=(
                key + " x1e"), color=f_value_props[key][0])
        else:
            plt.plot(scaled_market_data[i][:NB_INSTRUCTIONS], label=(
                key + " x1e"))
else:
    for (i, key) in enumerate(value_names):
        if f_value_props[key][0] != "":
            plt.plot(scaled_market_data[i], label=(
                key + " x1e" + str(scaling_factors[i])), color=f_value_props[key][0])
        else:
            plt.plot(scaled_market_data[i], label=(
                key + " x1e"))


plt.legend(prop={'size': 15})
plt.show()  # block=False)
# plt.savefig("../log/graph_{}.png".format(date_time), dpi=440)

#  gc_list_lengths: [0], page_full_ratios: [[], [0.0, 0.0, 0.0, 0.0, 0.0]]
